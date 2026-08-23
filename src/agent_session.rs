//! Shared post-register agent session loop.
//!
//! Both long-lived agent transports — WebSocket (`agent_ws`) and custom QUIC
//! (`agent_quic`) — run the *same* session once a connection is registered:
//! a request pump that drains the shared registry queue and pushes `Request`
//! envelopes, a reader loop that dispatches `Result`/`PersistentShellResult`/
//! `JobUpdate`/`Ping`/`Pong`/`RuntimeMetadata`/`Goodbye` envelopes into the
//! connection-scoped registry lease, and a teardown that stops the pump, joins
//! the writer, and reconciles the disconnect. The only differences are the
//! wire I/O (how a frame is read or written) and a log label. This module owns
//! that shared loop; the two
//! transport modules own transport-specific registration, auth, and I/O.
//!
//! Connection-lease scoping: every registry call below takes the
//! `connection_id` so a stale same-instance reconnect cannot consume or
//! refresh the newer connection's lease. This mirrors the polling transport's
//! `*_for_connection` discipline.

use crate::auth::{AuthContext, SCOPE_AGENT_REGISTER};
use crate::shell_client::{
    effective_register_owner, enforce_register_owner, require_agent_transport_scope,
    ShellClientRegistry,
};
use crate::shell_protocol::{AgentEnvelope, ShellAgentPollRequest, ShellClientRegisterRequest};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;

/// Channel capacity for outgoing envelopes (requests + pongs). Provides
/// backpressure if the agent reads slowly. Shared by both transports.
pub(crate) const OUTGOING_CHANNEL_CAPACITY: usize = 64;

/// Bound writer teardown after the reader/session side has already ended.
const STREAM_WRITER_JOIN_TIMEOUT: Duration = Duration::from_secs(1);

/// Transport writer completion without retaining an unsent envelope or error body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriterExit {
    ChannelClosed,
    TransportFailed,
}

/// Error from [`register_session_prelude`]: the agent transport boundary
/// rejected the register. Both variants surface to the wire as the
/// `register_forbidden` error code; they are distinguished only so the caller
/// can log which gate failed.
#[derive(Debug)]
pub(crate) enum RegisterPreludeError {
    /// `require_agent_transport_scope` rejected the caller (wrong scope / not a
    /// bootstrap or agent token).
    ForbiddenScope(String),
    /// `enforce_register_owner` rejected the client_id/owner binding.
    ForbiddenOwner(String),
}

impl RegisterPreludeError {
    /// Wire error code shared by both prelude gates.
    pub(crate) const CODE: &'static str = "register_forbidden";

    /// The human-readable message to send to the agent.
    pub(crate) fn message(&self) -> &str {
        match self {
            Self::ForbiddenScope(m) | Self::ForbiddenOwner(m) => m,
        }
    }
}

/// Enforce the agent transport boundary shared by the WebSocket and QUIC
/// register handlers, and resolve the effective owner onto `register_payload`.
///
/// This is the transport-neutral half of registration: it mirrors the polling
/// register handler's checks — bootstrap may register any owner, an agent
/// token may register only when its `allowed_client_id` matches and its owner
/// matches the requested owner (or fills it in when absent), a direct shared
/// key registers into its hash group with no trusted owner, and other user
/// tokens are rejected. It stops **before** any wire I/O: on failure it returns
/// the reason and lets the caller send its transport-specific error envelope
/// and log the cause. On success `register_payload.owner` is set to the
/// effective owner and the caller proceeds to mutate the registry.
pub(crate) fn register_session_prelude(
    auth: Option<&AuthContext>,
    register_payload: &mut ShellClientRegisterRequest,
) -> Result<(), RegisterPreludeError> {
    if let Err(e) = require_agent_transport_scope(auth, SCOPE_AGENT_REGISTER) {
        return Err(RegisterPreludeError::ForbiddenScope(e));
    }
    if let Err(e) = enforce_register_owner(
        auth,
        &register_payload.client_id,
        register_payload.owner.as_deref(),
    ) {
        return Err(RegisterPreludeError::ForbiddenOwner(e));
    }
    register_payload.owner = effective_register_owner(auth, register_payload.owner.as_deref());
    Ok(())
}

/// Outcome of a single inbound read on the shared reader loop.
#[derive(Debug)]
pub(crate) enum RecvOutcome {
    /// A decoded envelope ready to dispatch.
    Envelope(AgentEnvelope),
    /// A frame was consumed but yielded no envelope (e.g. a non-text
    /// WebSocket frame, or a malformed envelope that was logged and skipped).
    /// The reader loop continues.
    Skip,
    /// The peer closed the connection or a fatal read error occurred. The
    /// transport logs the cause itself; the reader loop stops.
    Closed,
}

/// Transport-neutral inbound reader. Implementations wrap a WebSocket
/// `StreamExt` stream or a QUIC `RecvStream` and translate wire reads into
/// [`RecvOutcome`]s, logging transport-specific errors themselves.
pub(crate) trait AgentReader {
    async fn recv(&mut self) -> RecvOutcome;
}

/// Shared session context handed to [`run_agent_session`] after a transport
/// has authenticated, registered, and acknowledged the agent.
pub(crate) struct SessionContext<'a> {
    pub(crate) registry: &'a Arc<ShellClientRegistry>,
    pub(crate) client_id: &'a str,
    pub(crate) agent_instance_id: &'a str,
    pub(crate) connection_id: &'a str,
    pub(crate) notify: Arc<Notify>,
    /// Log label: `"websocket"` or `"quic"`.
    pub(crate) transport_label: &'static str,
}

/// Drive the post-register session to completion: request pump, reader-loop
/// dispatch, and teardown.
///
/// The caller owns the transport-specific **writer task** (`writer_task`),
/// which drains `out_tx` (an `AgentEnvelope` mpsc) onto the wire; this function
/// owns the **pump** (which feeds `out_tx`) and the **reader loop**. On exit
/// the pump is aborted, `out_tx` is dropped (so the writer flushes and exits),
/// the writer is joined, and the disconnect is reconciled according to the
/// registered capability: reconciliation-capable runners enter the bounded
/// `recovering` state, while legacy runners become `lost`; the client then
/// decays to stale/offline.
///
/// Returns when either direction becomes unusable: the reader closes/fails,
/// the peer sends `Goodbye`, or the writer task exits unexpectedly. The caller
/// is responsible for any post-teardown transport logging.
pub(crate) async fn run_agent_session(
    ctx: SessionContext<'_>,
    out_tx: mpsc::Sender<AgentEnvelope>,
    mut reader: impl AgentReader,
    writer_task: JoinHandle<WriterExit>,
) {
    let SessionContext {
        registry,
        client_id,
        agent_instance_id,
        connection_id,
        notify,
        transport_label,
    } = ctx;

    // Request pump: drain the shared registry queue for this connection and
    // push Request envelopes. Waits on the notifier when idle. This is the
    // only consumer of the queue for this connection; polling agents use the
    // HTTP poll endpoint against the same queue.
    //
    // The pump is bound to this concrete connection's lease. A same-instance
    // reconnect installs a new connection_id; once this connection loses the
    // lease, the scoped poll rejects it before dequeuing, so an older socket
    // cannot steal requests that belong to the new connection. On rejection
    // the pump stops rather than falling back to an unscoped poll or retrying.
    let pump_tx = out_tx.clone();
    let pump_registry = Arc::clone(registry);
    let pump_client_id = client_id.to_string();
    let pump_instance_id = agent_instance_id.to_string();
    let pump_connection_id = connection_id.to_string();
    let pump_notify = Arc::clone(&notify);
    let pump_task = tokio::spawn(async move {
        loop {
            // Create the notified future before polling so an enqueue that
            // happens while poll returns None is not missed.
            let notified = pump_notify.notified();
            let poll_req = ShellAgentPollRequest {
                client_id: pump_client_id.clone(),
                agent_instance_id: pump_instance_id.clone(),
                projects: None,
            };
            match pump_registry
                .poll_for_connection(poll_req, &pump_connection_id)
                .await
            {
                Ok(Some(request)) => {
                    // Do not log the SendError: its Debug representation can
                    // include the unsent request envelope, which may carry
                    // command/stdin payloads. The closed channel is enough.
                    if pump_tx
                        .send(AgentEnvelope::Request { request })
                        .await
                        .is_err()
                    {
                        tracing::debug!(
                            client_id = %pump_client_id,
                            "agent {} pump send channel closed; stopping pump",
                            transport_label
                        );
                        break;
                    }
                }
                Ok(None) => {
                    notified.await;
                }
                Err(e) => {
                    tracing::warn!(
                        client_id = %pump_client_id,
                        error = %e,
                        "agent {} pump poll failed; stopping pump",
                        transport_label
                    );
                    break;
                }
            }
        }
    });

    // Bidirectional health: the session is authoritative only while both read
    // and write directions are alive. Observe the writer directly so a dead
    // outbound transport cannot remain registered behind a pending reader.
    let mut writer_task = writer_task;
    let mut writer_observed = false;
    loop {
        tokio::select! {
            writer = &mut writer_task => {
                writer_observed = true;
                let reason_code = match writer {
                    Ok(WriterExit::ChannelClosed) => "writer_channel_closed",
                    Ok(WriterExit::TransportFailed) => "writer_transport_failed",
                    Err(error) if error.is_panic() => "writer_task_panicked",
                    Err(_) => "writer_task_cancelled",
                };
                tracing::debug!(
                    client_id = client_id,
                    reason_code,
                    "agent {} writer ended; terminating session",
                    transport_label
                );
                break;
            }
            received = reader.recv() => {
                match received {
                    RecvOutcome::Envelope(env) => {
                        let is_goodbye = matches!(&env, AgentEnvelope::Goodbye { .. });
                        dispatch_inbound(
                            env,
                            registry,
                            client_id,
                            agent_instance_id,
                            connection_id,
                            &out_tx,
                            transport_label,
                        )
                        .await;
                        if is_goodbye {
                            break;
                        }
                    }
                    RecvOutcome::Skip => continue,
                    RecvOutcome::Closed => break,
                }
            }
        }
    }

    // Teardown: stop the pump, close the writer feed, bound any remaining writer
    // join, then reconcile the exact connection lease. A stale writer completion
    // is harmless because reconciliation is fenced by client/instance/connection.
    pump_task.abort();
    drop(out_tx);
    if !writer_observed {
        match tokio::time::timeout(STREAM_WRITER_JOIN_TIMEOUT, &mut writer_task).await {
            Ok(Ok(WriterExit::ChannelClosed)) => {}
            Ok(Ok(WriterExit::TransportFailed)) => {
                tracing::debug!(
                    client_id = client_id,
                    reason_code = "writer_transport_failed_during_teardown",
                    "agent {} writer failed during teardown",
                    transport_label
                );
            }
            Ok(Err(error)) => {
                let reason_code = if error.is_panic() {
                    "writer_task_panicked_during_teardown"
                } else {
                    "writer_task_cancelled_during_teardown"
                };
                tracing::debug!(
                    client_id = client_id,
                    reason_code,
                    "agent {} writer join failed during teardown",
                    transport_label
                );
            }
            Err(_) => {
                tracing::debug!(
                    client_id = client_id,
                    reason_code = "writer_join_timeout",
                    "agent {} writer join timed out; aborting writer",
                    transport_label
                );
                writer_task.abort();
            }
        }
    }
    registry
        .reconcile_disconnect_for_connection(client_id, agent_instance_id, connection_id)
        .await;
}

/// Dispatch one inbound envelope into the connection-scoped registry lease.
async fn dispatch_inbound(
    env: AgentEnvelope,
    registry: &Arc<ShellClientRegistry>,
    client_id: &str,
    agent_instance_id: &str,
    connection_id: &str,
    out_tx: &mpsc::Sender<AgentEnvelope>,
    transport_label: &'static str,
) {
    match env {
        AgentEnvelope::Result { payload } => {
            if payload.result.client_id != client_id
                || payload.result.agent_instance_id != agent_instance_id
            {
                tracing::warn!(
                    client_id = client_id,
                    "agent {} result rejected: envelope identity does not match registered connection",
                    transport_label
                );
                return;
            }
            // `complete_for_connection` refreshes `last_seen` internally only
            // when this connection still holds the lease; a late result on a
            // stale same-instance connection is still applied but does not
            // revive the new connection's liveness.
            if let Err(e) = registry
                .complete_for_connection(payload, connection_id)
                .await
            {
                tracing::warn!(
                    client_id = client_id,
                    error = %e,
                    "agent {} result rejected",
                    transport_label
                );
            }
        }
        AgentEnvelope::PersistentShellResult { payload } => {
            if payload.client_id != client_id || payload.agent_instance_id != agent_instance_id {
                tracing::warn!(
                    client_id = client_id,
                    "agent {} persistent shell result rejected: envelope identity does not match registered connection",
                    transport_label
                );
                return;
            }
            if let Err(e) = registry
                .complete_persistent_shell_for_connection(payload, connection_id)
                .await
            {
                tracing::warn!(
                    client_id = client_id,
                    error = %e,
                    "agent {} persistent shell result rejected",
                    transport_label
                );
            }
        }
        AgentEnvelope::JobUpdate { payload } => {
            if payload.client_id != client_id || payload.agent_instance_id != agent_instance_id {
                tracing::warn!(
                    client_id = client_id,
                    "agent {} job_update rejected: envelope identity does not match registered connection",
                    transport_label
                );
                return;
            }
            if let Err(e) = registry
                .update_job_for_connection(payload, connection_id)
                .await
            {
                tracing::warn!(
                    client_id = client_id,
                    error = %e,
                    "agent {} job_update rejected",
                    transport_label
                );
            }
        }
        AgentEnvelope::Ping { ts } => {
            // Keepalive: refresh liveness before replying so an idle agent (no
            // pending requests) is not aged out of the online window. Without
            // this touch a connected-but-idle agent decays to "stale" even
            // though its socket is healthy.
            if let Err(e) = registry
                .touch_client_for_connection(client_id, agent_instance_id, connection_id)
                .await
            {
                tracing::warn!(
                    client_id = client_id,
                    error = %e,
                    "agent {} ping liveness touch failed",
                    transport_label
                );
            }
            // Pong is best-effort: never block the reader if the outbound
            // channel is full (a slow agent must not stall inbound processing).
            // try_send drops the pong when saturated; the agent treats a
            // missing pong as a soft liveness signal, not a fatal error.
            if let Err(e) = out_tx.try_send(AgentEnvelope::Pong { ts }) {
                let reason = match e {
                    tokio::sync::mpsc::error::TrySendError::Full(_) => "full",
                    tokio::sync::mpsc::error::TrySendError::Closed(_) => "closed",
                };
                tracing::debug!(
                    client_id = client_id,
                    reason,
                    "agent {} pong send dropped",
                    transport_label
                );
            }
        }
        AgentEnvelope::Pong { .. } => {
            // Pong is a normal keepalive response. The server does not
            // currently originate Pings, but a Pong must still count as live
            // traffic so the client does not decay to stale, and must never be
            // treated as an unexpected envelope.
            if let Err(e) = registry
                .touch_client_for_connection(client_id, agent_instance_id, connection_id)
                .await
            {
                tracing::debug!(
                    client_id = client_id,
                    error = %e,
                    "agent {} pong liveness touch failed",
                    transport_label
                );
            }
        }
        AgentEnvelope::RuntimeMetadata { tool_providers } => {
            let _ = registry
                .update_tool_providers_for_connection(
                    client_id,
                    agent_instance_id,
                    connection_id,
                    Some(tool_providers),
                )
                .await;
        }
        AgentEnvelope::ProjectInventoryPage { page } => {
            match registry
                .apply_project_inventory_page_for_connection(
                    client_id,
                    agent_instance_id,
                    connection_id,
                    page,
                )
                .await
            {
                Ok(status) => {
                    // Bounded best-effort acknowledgement. A full outbound
                    // channel must not make project inventory a liveness fence;
                    // the Runner can restart the snapshot on reconnect.
                    let _ = out_tx.try_send(AgentEnvelope::ProjectInventoryStatus { status });
                }
                Err(error) => {
                    tracing::debug!(
                        client_id = client_id,
                        error = %error,
                        "agent project inventory page rejected by lease fence"
                    );
                }
            }
        }
        AgentEnvelope::ProjectInventoryStatus { .. } => {
            // Server-to-Runner only; ignore if a peer reflects it.
        }
        AgentEnvelope::Goodbye { reason } => {
            tracing::debug!(
                client_id = client_id,
                reason = reason.as_deref().unwrap_or("unspecified"),
                "agent {} sent goodbye",
                transport_label
            );
            registry
                .reconcile_disconnect_for_connection(client_id, agent_instance_id, connection_id)
                .await;
        }
        AgentEnvelope::Register { .. } => {
            // Ignore a redundant register mid-session.
        }
        other => {
            tracing::debug!(
                client_id = client_id,
                kind = other.kind(),
                "agent {} received unexpected envelope; ignoring",
                transport_label
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_client::TRANSPORT_WEBSOCKET;
    use crate::shell_protocol::{ShellClientCapabilities, ShellJobOpRequest};

    struct PendingReader;

    impl AgentReader for PendingReader {
        async fn recv(&mut self) -> RecvOutcome {
            std::future::pending::<RecvOutcome>().await
        }
    }

    fn streaming_registration(
        client_id: &str,
        agent_instance_id: &str,
    ) -> ShellClientRegisterRequest {
        let mut capabilities = ShellClientCapabilities::default();
        capabilities.async_jobs = true;
        capabilities.async_shell_jobs = true;
        ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(capabilities),
            projects: Some(Vec::new()),
            agent_protocol_version: Some("websocket-v1".to_string()),
            policy: None,
        }
    }

    #[tokio::test]
    async fn writer_failure_terminates_session_and_reconciles_active_job() {
        let registry = Arc::new(ShellClientRegistry::default());
        let notify = Arc::new(Notify::new());
        registry
            .register_streaming_session(
                streaming_registration("writer-fail", "inst-a"),
                None,
                "conn-a",
                TRANSPORT_WEBSOCKET,
                notify.clone(),
            )
            .await
            .unwrap();
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("writer-fail".to_string()),
                    cwd: None,
                    command: Some("sleep 1".to_string()),
                    timeout_secs: Some(1),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let (out_tx, _out_rx) = mpsc::channel(OUTGOING_CHANNEL_CAPACITY);
        let writer_task = tokio::spawn(async { WriterExit::TransportFailed });

        tokio::time::timeout(
            Duration::from_millis(250),
            run_agent_session(
                SessionContext {
                    registry: &registry,
                    client_id: "writer-fail",
                    agent_instance_id: "inst-a",
                    connection_id: "conn-a",
                    notify,
                    transport_label: "websocket",
                },
                out_tx,
                PendingReader,
                writer_task,
            ),
        )
        .await
        .expect("writer failure must terminate a pending reader session");

        let view = registry.get_client_view("writer-fail").await.unwrap();
        assert!(!view.connected);
        assert_eq!(registry.get_job(&job.job_id).await.unwrap().status, "lost");
    }

    #[tokio::test]
    async fn writer_task_panic_terminates_session() {
        let registry = Arc::new(ShellClientRegistry::default());
        let notify = Arc::new(Notify::new());
        registry
            .register_streaming_session(
                streaming_registration("writer-panic", "inst-a"),
                None,
                "conn-a",
                TRANSPORT_WEBSOCKET,
                notify.clone(),
            )
            .await
            .unwrap();
        let (out_tx, _out_rx) = mpsc::channel(OUTGOING_CHANNEL_CAPACITY);
        let writer_task = tokio::spawn(async { panic!("synthetic writer panic") });

        tokio::time::timeout(
            Duration::from_millis(250),
            run_agent_session(
                SessionContext {
                    registry: &registry,
                    client_id: "writer-panic",
                    agent_instance_id: "inst-a",
                    connection_id: "conn-a",
                    notify,
                    transport_label: "websocket",
                },
                out_tx,
                PendingReader,
                writer_task,
            ),
        )
        .await
        .expect("writer panic must terminate a pending reader session");

        assert!(
            !registry
                .get_client_view("writer-panic")
                .await
                .unwrap()
                .connected
        );
    }

    #[tokio::test]
    async fn stale_writer_failure_cannot_reconcile_replacement_connection() {
        let registry = Arc::new(ShellClientRegistry::default());
        let notify_a = Arc::new(Notify::new());
        registry
            .register_streaming_session(
                streaming_registration("writer-stale", "inst-a"),
                None,
                "conn-a",
                TRANSPORT_WEBSOCKET,
                notify_a.clone(),
            )
            .await
            .unwrap();
        registry
            .register_streaming_session(
                streaming_registration("writer-stale", "inst-a"),
                None,
                "conn-b",
                TRANSPORT_WEBSOCKET,
                Arc::new(Notify::new()),
            )
            .await
            .unwrap();
        let (out_tx, _out_rx) = mpsc::channel(OUTGOING_CHANNEL_CAPACITY);
        let writer_task = tokio::spawn(async { WriterExit::TransportFailed });

        tokio::time::timeout(
            Duration::from_millis(250),
            run_agent_session(
                SessionContext {
                    registry: &registry,
                    client_id: "writer-stale",
                    agent_instance_id: "inst-a",
                    connection_id: "conn-a",
                    notify: notify_a,
                    transport_label: "quic",
                },
                out_tx,
                PendingReader,
                writer_task,
            ),
        )
        .await
        .expect("stale writer failure must terminate without touching replacement");

        let replacement = registry
            .get_client_view_for_connection("writer-stale", "inst-a", "conn-b")
            .await
            .expect("replacement connection must remain authoritative");
        assert!(replacement.connected);
        assert_eq!(replacement.transport, TRANSPORT_WEBSOCKET);
    }
}
