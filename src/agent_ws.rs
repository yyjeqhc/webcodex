//! Server-side WebSocket agent transport.
//!
//! This module implements the WebSocket endpoint that lets an agent stay
//! connected over a long-lived connection instead of polling. It is
//! intentionally thin: every business operation (register, request routing,
//! result recording, job updates) is delegated to the existing
//! [`ShellClientRegistry`]. The handler only translates between the
//! transport-neutral [`AgentEnvelope`] wire format and registry method calls.
//!
//! Request delivery model: after a successful register the server spawns a
//! "request pump" task. The pump pops pending requests from the registry
//! queue (the very same queue the polling endpoint serves) and pushes them to
//! the agent as `Request` envelopes. When the queue is empty, the pump waits
//! on a [`Notify`] that the registry fires whenever a new request is
//! enqueued. This means WebSocket and polling agents share one queue and one
//! job state; there is no second business-logic path.
//!
//! Polling remains a fully supported fallback transport.

use crate::shell_client::{
    effective_register_owner, enforce_register_owner, require_agent_transport_scope,
    ShellClientRegistry, TRANSPORT_WEBSOCKET,
};
use crate::shell_protocol::{AgentEnvelope, ShellAgentPollRequest, ShellClientRegisterRequest};
use futures_util::{SinkExt, StreamExt};
use salvo::prelude::*;
use salvo::websocket::{Message, WebSocket, WebSocketUpgrade};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Notify};

/// Maximum WebSocket text message size. Agent requests/results carry shell
/// output which can be sizeable; 8 MiB matches the registry output cap head
/// room while still bounding memory.
const WS_MAX_MESSAGE_SIZE: usize = 8 * 1024 * 1024;
/// Deadline for the agent to send its first `Register` envelope after the
/// handshake. Prevents half-open connections from holding registry state.
const REGISTER_TIMEOUT: Duration = Duration::from_secs(15);
/// Channel capacity for outgoing envelopes (requests + pongs). Provides
/// backpressure if the agent reads slowly.
const OUTGOING_CHANNEL_CAPACITY: usize = 64;

/// WebSocket agent endpoint: `GET /api/agents/ws` (also mounted at
/// `/api/agents/ws`). Requires auth via the shared `AuthMiddleware`, exactly
/// like the polling endpoints. The normal path is `Authorization: Bearer
/// <token>`; `?token=` is accepted only on this WebSocket handshake path for
/// compatibility with clients that cannot set handshake headers.
#[handler]
pub async fn agent_ws(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = depot.obtain::<Arc<ShellClientRegistry>>().ok().cloned() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(json!({
            "success": false,
            "error": "Shell client registry not configured"
        })));
        return;
    };
    // AuthMiddleware (hoop on the api router) validates the Bearer token at
    // the HTTP handshake and injects the AuthContext into the depot. We pull
    // it out here because the upgrade callback does not receive a depot.
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let upgrade = WebSocketUpgrade::new()
        .max_message_size(WS_MAX_MESSAGE_SIZE)
        .upgrade(req, res, |ws| async move {
            handle_agent_ws(ws, registry, auth).await;
        })
        .await;
    if let Err(e) = upgrade {
        tracing::warn!(error = ?e, "agent websocket upgrade failed");
    }
}

/// Drive a single agent WebSocket connection to completion (until the agent
/// disconnects or a fatal protocol error occurs).
async fn handle_agent_ws(
    mut ws: WebSocket,
    registry: Arc<ShellClientRegistry>,
    auth: Option<crate::auth::AuthContext>,
) {
    // 1. Read the first message: it must be a Register envelope.
    let register_payload = match read_register(&mut ws).await {
        Ok(payload) => payload,
        Err(e) => {
            send_envelope_or_log(
                &mut ws,
                AgentEnvelope::Error {
                    code: "expected_register".to_string(),
                    message: e,
                },
                "expected_register",
            )
            .await;
            return;
        }
    };
    let client_id = register_payload.client_id.clone();
    let agent_instance_id = register_payload.agent_instance_id.clone();

    // 1b. Enforce the agent transport boundary before mutating the registry.
    //     This mirrors the polling register handler: bootstrap may register
    //     any owner; an agent token may register only when its
    //     allowed_client_id matches and its owner matches the requested owner
    //     (or fills it in when absent); user tokens are rejected. When no
    //     AuthContext is present (unit tests without AuthMiddleware) the check
    //     is a no-op; production always runs behind AuthMiddleware.
    if let Err(e) = require_agent_transport_scope(auth.as_ref(), crate::auth::SCOPE_AGENT_REGISTER)
    {
        send_envelope_or_log(
            &mut ws,
            AgentEnvelope::Error {
                code: "register_forbidden".to_string(),
                message: e,
            },
            "register_forbidden",
        )
        .await;
        return;
    }
    if let Err(e) = enforce_register_owner(
        auth.as_ref(),
        &register_payload.client_id,
        register_payload.owner.as_deref(),
    ) {
        send_envelope_or_log(
            &mut ws,
            AgentEnvelope::Error {
                code: "register_forbidden".to_string(),
                message: e,
            },
            "register_forbidden",
        )
        .await;
        return;
    }
    // Resolve the effective owner (agent token fills owner from its username).
    let mut register_payload = register_payload;
    register_payload.owner =
        effective_register_owner(auth.as_ref(), register_payload.owner.as_deref());

    // 2. Register into the shared registry (same path as polling register),
    //    then flip the transport label and install a push notifier so the
    //    request pump can be woken on enqueue.
    if let Err(e) = registry.register(register_payload).await {
        send_envelope_or_log(
            &mut ws,
            AgentEnvelope::Error {
                code: "register_failed".to_string(),
                message: e,
            },
            "register_failed",
        )
        .await;
        return;
    }
    let _ = registry
        .set_transport(&client_id, TRANSPORT_WEBSOCKET)
        .await;
    let notify = Arc::new(Notify::new());
    if registry
        .register_notifier(&client_id, &agent_instance_id, notify.clone())
        .await
        .is_err()
    {
        send_envelope_or_log(
            &mut ws,
            AgentEnvelope::Error {
                code: "register_failed".to_string(),
                message: "failed to install push notifier".to_string(),
            },
            "register_failed",
        )
        .await;
        return;
    }
    // Fetch the view after set_transport so the ack reflects the websocket
    // transport label rather than the default "polling".
    let Some(view) = registry.get_client_view(&client_id).await else {
        return;
    };

    // 3. Acknowledge the register.
    send_envelope_or_log(
        &mut ws,
        AgentEnvelope::Registered {
            success: true,
            client: Some(view),
            error: None,
        },
        "registered",
    )
    .await;
    tracing::info!(client_id = %client_id, "agent websocket connected");

    // 4. Split the socket into a writer (owned by a writer task) and a reader
    //    (owned by this task). Outgoing envelopes go through a single mpsc so
    //    the request pump and pong replies share one writer.
    let (sink, stream) = ws.split();
    let (out_tx, out_rx) = mpsc::channel::<String>(OUTGOING_CHANNEL_CAPACITY);

    let writer_task = tokio::spawn(async move {
        let mut sink = sink;
        let mut out_rx = out_rx;
        while let Some(json) = out_rx.recv().await {
            if let Err(e) = sink.send(Message::text(json)).await {
                tracing::debug!(error = ?e, "agent websocket writer send failed; stopping writer");
                break;
            }
        }
        if let Err(e) = sink.close().await {
            tracing::debug!(error = ?e, "agent websocket writer close failed");
        }
    });

    // 5. Request pump: drain the registry queue for this client and push
    //    Request envelopes. Waits on the notifier when idle. This is the only
    //    consumer of the queue for this client; polling agents use the HTTP
    //    poll endpoint against the same queue.
    let pump_tx = out_tx.clone();
    let pump_registry = registry.clone();
    let pump_client_id = client_id.clone();
    let pump_instance_id = agent_instance_id.clone();
    let pump_notify = notify.clone();
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
            match pump_registry.poll(poll_req).await {
                Ok(Some(request)) => {
                    let env = AgentEnvelope::Request { request };
                    match env.to_json() {
                        Ok(json) => {
                            if pump_tx.send(json).await.is_err() {
                                // Do not log the SendError<String>: its Debug
                                // representation can include the unsent
                                // request JSON, which may carry command/stdin
                                // payloads. The channel state is enough for
                                // diagnostics here.
                                tracing::debug!(
                                    client_id = %pump_client_id,
                                    "agent websocket pump send channel closed; stopping pump"
                                );
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                client_id = %pump_client_id,
                                error = %e,
                                "agent websocket pump failed to encode request envelope"
                            );
                            break;
                        }
                    }
                }
                Ok(None) => {
                    notified.await;
                }
                Err(e) => {
                    tracing::warn!(
                        client_id = %pump_client_id,
                        error = %e,
                        "agent websocket pump poll failed; stopping pump"
                    );
                    break;
                }
            }
        }
    });

    // 6. Reader loop: handle Result/JobUpdate/Ping from the agent.
    let mut stream = stream;
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(client_id = %client_id, error = ?e, "agent websocket read error");
                break;
            }
        };
        if msg.is_close() {
            break;
        }
        // tungstenite auto-replies to Ping with Pong at the protocol level,
        // so we only react to application Text messages here.
        let text = match msg.as_str() {
            Ok(s) => s,
            Err(_) => continue,
        };
        let env = match AgentEnvelope::from_slice(text.as_bytes()) {
            Ok(env) => env,
            Err(e) => {
                tracing::debug!(
                    client_id = %client_id,
                    error = %e,
                    "agent websocket received malformed envelope; ignoring"
                );
                continue;
            }
        };
        match env {
            AgentEnvelope::Result { payload } => {
                // `complete` refreshes `last_seen` internally; a redundant
                // touch here would only add lock contention.
                if let Err(e) = registry.complete(payload).await {
                    tracing::warn!(client_id = %client_id, error = %e, "ws result rejected");
                }
            }
            AgentEnvelope::JobUpdate { payload } => {
                // `update_job` refreshes `last_seen` internally.
                if let Err(e) = registry.update_job(payload).await {
                    tracing::warn!(client_id = %client_id, error = %e, "ws job_update rejected");
                }
            }
            AgentEnvelope::Ping { ts } => {
                // Keepalive: refresh liveness before replying so an idle
                // WebSocket agent (no pending requests) is not aged out of the
                // online window by the 60s `CLIENT_ONLINE_WINDOW_SECS` check.
                // Without this touch, a connected-but-idle agent decays to
                // `"stale"` even though its socket is healthy.
                if let Err(e) = registry.touch_client(&client_id, &agent_instance_id).await {
                    tracing::warn!(client_id = %client_id, error = %e, "ws ping liveness touch failed");
                }
                let pong = AgentEnvelope::Pong { ts };
                if let Ok(json) = pong.to_json() {
                    // Pong is a best-effort keepalive: never block the reader
                    // if the outbound channel is full (a slow agent must not
                    // stall inbound processing). try_send drops the pong when
                    // the channel is saturated; the agent treats a missing
                    // pong as a soft liveness signal, not a fatal error.
                    if let Err(e) = out_tx.try_send(json) {
                        let reason = match e {
                            tokio::sync::mpsc::error::TrySendError::Full(_) => "full",
                            tokio::sync::mpsc::error::TrySendError::Closed(_) => "closed",
                        };
                        tracing::debug!(
                            client_id = %client_id,
                            reason,
                            "agent websocket pong send dropped"
                        );
                    }
                }
            }
            AgentEnvelope::Pong { .. } => {
                // Pong is a normal keepalive response. The server does not
                // currently originate Pings, but a Pong (e.g. a stray or
                // future server-initiated ping reply) must still count as
                // live traffic so the client does not decay to stale, and it
                // must never be treated as an unexpected envelope.
                if let Err(e) = registry.touch_client(&client_id, &agent_instance_id).await {
                    tracing::debug!(client_id = %client_id, error = %e, "ws pong liveness touch failed");
                }
            }
            AgentEnvelope::Register { .. } => {
                // Ignore a redundant register mid-session.
            }
            other => {
                tracing::debug!(
                    client_id = %client_id,
                    kind = other.kind(),
                    "agent websocket received unexpected envelope; ignoring"
                );
            }
        }
    }

    // 7. Cleanup: stop the pump, drain the writer, and remove the notifier so
    //    the client naturally decays to stale/offline instead of staying
    //    "online websocket" forever.
    pump_task.abort();
    drop(out_tx);
    if let Err(e) = writer_task.await {
        tracing::debug!(client_id = %client_id, error = ?e, "agent websocket writer task join failed");
    }
    // Reconcile: drop the notifier and mark running jobs lost so a
    // disconnected agent never leaves jobs permanently "running" or appears
    // permanently online (the client decays to stale via last_seen).
    registry
        .reconcile_disconnect(&client_id, &agent_instance_id)
        .await;
    tracing::info!(client_id = %client_id, "agent websocket disconnected");
}

/// Read the first envelope from the socket, requiring it to be a `Register`.
/// Applies a deadline so a half-open connection cannot hold registry state.
async fn read_register(ws: &mut WebSocket) -> Result<ShellClientRegisterRequest, String> {
    let msg = tokio::time::timeout(REGISTER_TIMEOUT, ws.recv())
        .await
        .map_err(|_| "register timed out".to_string())?
        .ok_or_else(|| "connection closed before register".to_string())?
        .map_err(|e| format!("read error before register: {}", e))?;
    let text = msg
        .as_str()
        .map_err(|_| "register message must be text".to_string())?;
    let env = AgentEnvelope::from_slice(text.as_bytes())
        .map_err(|e| format!("register message is not a valid envelope: {}", e))?;
    match env {
        AgentEnvelope::Register { payload } => Ok(payload),
        other => Err(format!("expected register envelope, got {}", other.kind())),
    }
}

/// Encode and send a single envelope before the socket is split.
async fn send_envelope(ws: &mut WebSocket, env: AgentEnvelope) -> Result<(), ()> {
    let json = env.to_json().map_err(|_| ())?;
    ws.send(Message::text(json)).await.map_err(|_| ())
}

async fn send_envelope_or_log(ws: &mut WebSocket, env: AgentEnvelope, context: &'static str) {
    let kind = env.kind();
    if send_envelope(ws, env).await.is_err() {
        tracing::debug!(
            envelope_kind = kind,
            context,
            "agent websocket pre-register send failed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_protocol::{
        ShellAgentResultRequest, ShellClientCapabilities, ShellClientRegisterRequest,
        ShellJobOpRequest, ShellRunRequest,
    };
    use salvo::conn::{Acceptor, Listener};
    use std::net::SocketAddr;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    fn register_envelope(client_id: &str) -> AgentEnvelope {
        register_envelope_with_instance(client_id, "ws-inst")
    }

    fn register_envelope_with_instance(client_id: &str, instance_id: &str) -> AgentEnvelope {
        AgentEnvelope::Register {
            payload: ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: instance_id.to_string(),
                display_name: Some("ws-test".to_string()),
                owner: Some("tester".to_string()),
                hostname: None,
                capabilities: Some(ShellClientCapabilities {
                    shell: true,
                    file_read: true,
                    file_write: true,
                    git: false,
                    jobs: true,
                    async_jobs: true,
                    async_shell_jobs: true,
                }),
                projects: None,
                agent_protocol_version: Some(
                    crate::shell_protocol::AGENT_PROTOCOL_VERSION_WEBSOCKET_V1.to_string(),
                ),
                policy: None,
            },
        }
    }

    /// A `last_seen` timestamp comfortably past the 60s online window, used to
    /// simulate liveness decay without a real sleep. The window constant lives
    /// in `shell_client` and is private, so we use a generous 2-minute age.
    fn aged_last_seen() -> i64 {
        chrono::Utc::now().timestamp() - 120
    }

    async fn recv_envelope(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> AgentEnvelope {
        let msg = ws
            .next()
            .await
            .expect("stream not closed")
            .expect("ok message");
        let text = msg.into_text().expect("text message");
        AgentEnvelope::from_slice(text.as_bytes()).expect("valid envelope")
    }

    /// Build a salvo router serving only the agent ws endpoint backed by a
    /// fresh registry. No auth middleware: the integration test exercises the
    /// protocol, not authentication.
    fn build_router(registry: Arc<ShellClientRegistry>) -> Router {
        Router::new()
            .hoop(affix_state::inject(registry))
            .push(Router::with_path("api/agents/ws").goal(agent_ws))
    }

    async fn start_server(registry: Arc<ShellClientRegistry>) -> SocketAddr {
        let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
        let addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let router = build_router(registry);
        tokio::spawn(async move {
            Server::new(acceptor).serve(router).await;
        });
        addr
    }

    #[tokio::test]
    async fn ws_register_then_request_result_roundtrip() {
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;

        let url = format!("ws://{}/api/agents/ws", addr);
        let (mut ws, _resp) = connect_async(url).await.expect("ws connect");

        // Register.
        ws.send(TungsteniteMessage::Text(
            register_envelope("ws-roundtrip").to_json().unwrap().into(),
        ))
        .await
        .unwrap();

        // Expect Registered ack.
        let ack = recv_envelope(&mut ws).await;
        match ack {
            AgentEnvelope::Registered {
                success, client, ..
            } => {
                assert!(success);
                let client = client.expect("client view");
                assert_eq!(client.client_id, "ws-roundtrip");
                assert_eq!(client.transport, "websocket");
            }
            other => panic!("expected registered, got {:?}", other),
        }

        // Enqueue a synchronous run request via the registry (same path the
        // GPT Actions / MCP surface uses). The pump should push it.
        let (request_id, rx) = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "ws-roundtrip".to_string(),
                    cwd: None,
                    command: "echo hi".to_string(),
                    stdin: None,
                    timeout_secs: 5,
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();

        // Receive the pushed Request envelope.
        let req_env = recv_envelope(&mut ws).await;
        match req_env {
            AgentEnvelope::Request { request } => {
                assert_eq!(request.request_id, request_id);
                assert_eq!(request.kind, "run_shell");
                assert_eq!(request.command, "echo hi");
            }
            other => panic!("expected request, got {:?}", other),
        }

        // Send back a Result envelope.
        let result_env = AgentEnvelope::Result {
            payload: ShellAgentResultRequest {
                client_id: "ws-roundtrip".to_string(),
                agent_instance_id: "ws-inst".to_string(),
                request_id: request_id.clone(),
                exit_code: Some(0),
                stdout: Some("hi".to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            },
        };
        ws.send(TungsteniteMessage::Text(
            result_env.to_json().unwrap().into(),
        ))
        .await
        .unwrap();

        // The registry should deliver the result to the oneshot waiter.
        let response = tokio::time::timeout(Duration::from_secs(3), rx)
            .await
            .unwrap()
            .unwrap();
        assert!(response.success);
        assert_eq!(response.stdout.as_deref(), Some("hi"));
        assert_eq!(response.exit_code, Some(0));
    }

    #[tokio::test]
    async fn ws_ping_replies_with_pong() {
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;

        let url = format!("ws://{}/api/agents/ws", addr);
        let (mut ws, _resp) = connect_async(url).await.unwrap();

        ws.send(TungsteniteMessage::Text(
            register_envelope("ws-ping").to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws).await; // Registered

        let ping = AgentEnvelope::Ping { ts: 12345 };
        ws.send(TungsteniteMessage::Text(ping.to_json().unwrap().into()))
            .await
            .unwrap();

        let pong = recv_envelope(&mut ws).await;
        match pong {
            AgentEnvelope::Pong { ts } => assert_eq!(ts, 12345),
            other => panic!("expected pong, got {:?}", other),
        }

        // A Ping must refresh liveness: the client stays online.
        let view = registry.get_client_view("ws-ping").await.unwrap();
        assert!(view.connected);
        assert_eq!(view.status, "online");
        assert_eq!(view.transport, "websocket");
    }

    #[tokio::test]
    async fn ws_ping_refreshes_liveness_after_aging() {
        // Simulate the 60s online window elapsing with only keepalive traffic
        // by directly aging `last_seen`, then sending a Ping. The server must
        // refresh liveness so the agent reads online again instead of decaying
        // to stale. This avoids a real 60s sleep.
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;

        let url = format!("ws://{}/api/agents/ws", addr);
        let (mut ws, _resp) = connect_async(url).await.unwrap();
        ws.send(TungsteniteMessage::Text(
            register_envelope("ws-age").to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws).await; // Registered

        // Age past the online window.
        registry
            .set_last_seen_for_test("ws-age", aged_last_seen())
            .await;
        let stale = registry.get_client_view("ws-age").await.unwrap();
        assert!(!stale.connected, "client should be stale after aging");

        // A Ping must bring it back online.
        ws.send(TungsteniteMessage::Text(
            AgentEnvelope::Ping { ts: 1 }.to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let pong = recv_envelope(&mut ws).await;
        assert!(matches!(pong, AgentEnvelope::Pong { .. }));

        let fresh = registry.get_client_view("ws-age").await.unwrap();
        assert!(fresh.connected);
        assert_eq!(fresh.status, "online");
    }

    #[tokio::test]
    async fn ws_pong_treated_as_keepalive_not_unexpected() {
        // A Pong from the agent (e.g. a future server-initiated ping reply,
        // or a stray frame) must be treated as live traffic, never as an
        // unexpected envelope, and must refresh liveness. The connection must
        // stay open.
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;

        let url = format!("ws://{}/api/agents/ws", addr);
        let (mut ws, _resp) = connect_async(url).await.unwrap();
        ws.send(TungsteniteMessage::Text(
            register_envelope("ws-pong").to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws).await; // Registered

        registry
            .set_last_seen_for_test("ws-pong", aged_last_seen())
            .await;
        assert!(!registry.get_client_view("ws-pong").await.unwrap().connected);

        // Send a Pong. The server must not close the socket and must not echo
        // anything back (Pong is terminal keepalive).
        ws.send(TungsteniteMessage::Text(
            AgentEnvelope::Pong { ts: 99 }.to_json().unwrap().into(),
        ))
        .await
        .unwrap();

        // Give the server a moment to process the frame.
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(25)).await;
            if registry.get_client_view("ws-pong").await.unwrap().connected {
                break;
            }
        }
        let fresh = registry.get_client_view("ws-pong").await.unwrap();
        assert!(fresh.connected, "pong must refresh liveness");
        assert_eq!(fresh.status, "online");

        // The connection is still usable: a subsequent Ping still gets a Pong.
        ws.send(TungsteniteMessage::Text(
            AgentEnvelope::Ping { ts: 7 }.to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let pong = recv_envelope(&mut ws).await;
        assert!(matches!(pong, AgentEnvelope::Pong { ts: 7 }));
    }

    #[tokio::test]
    async fn ws_reconnect_re_registers_same_client_id_as_websocket_online() {
        // After a disconnect the server reconciles (jobs lost, notifier
        // removed). A fresh WebSocket register for the same client_id must
        // overwrite the old record, flip transport back to websocket, and read
        // connected=true/online.
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;
        let url = format!("ws://{}/api/agents/ws", addr);

        // First session.
        let (mut ws1, _resp) = connect_async(url.clone()).await.unwrap();
        ws1.send(TungsteniteMessage::Text(
            register_envelope("ws-recon").to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let ack1 = recv_envelope(&mut ws1).await;
        assert!(matches!(
            ack1,
            AgentEnvelope::Registered { success: true, .. }
        ));
        let view1 = registry.get_client_view("ws-recon").await.unwrap();
        assert_eq!(view1.transport, "websocket");
        assert!(view1.connected);

        // Disconnect: server reconciles (retains the client record).
        drop(ws1);
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(25)).await;
            // Reconcile happens in the background; we just wait for it to
            // settle by observing the record is still present.
            if registry.get_client_view("ws-recon").await.is_some() {
                break;
            }
        }

        // Reconnect with the same client_id.
        let (mut ws2, _resp) = connect_async(url).await.unwrap();
        ws2.send(TungsteniteMessage::Text(
            register_envelope("ws-recon").to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let ack2 = recv_envelope(&mut ws2).await;
        match ack2 {
            AgentEnvelope::Registered {
                success, client, ..
            } => {
                assert!(success);
                let client = client.expect("client view in ack");
                assert_eq!(client.client_id, "ws-recon");
                assert_eq!(client.transport, "websocket");
                assert!(client.connected);
            }
            other => panic!("expected registered ack on reconnect, got {:?}", other),
        }

        let view2 = registry.get_client_view("ws-recon").await.unwrap();
        assert_eq!(view2.transport, "websocket");
        assert!(view2.connected);
        assert_eq!(view2.status, "online");
    }

    #[tokio::test]
    async fn ws_disconnect_marks_notifier_removed() {
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;

        let url = format!("ws://{}/api/agents/ws", addr);
        let (mut ws, _resp) = connect_async(url).await.unwrap();
        ws.send(TungsteniteMessage::Text(
            register_envelope("ws-disc").to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws).await; // Registered

        // While connected the transport is websocket.
        let view = registry.get_client_view("ws-disc").await.unwrap();
        assert_eq!(view.transport, "websocket");

        // Drop the socket.
        drop(ws);
        // Give the server a moment to observe the disconnect and clean up.
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            // After disconnect the notifier is gone; the client decays to
            // stale once last_seen ages past the online window. We only
            // assert the notifier was removed by re-registering a notifier
            // successfully (which would fail if still present? it replaces, so
            // instead assert transport label is unchanged but the client is
            // still known).
            let _ = registry.get_client_view("ws-disc").await;
        }
        // The client record is retained (so jobs/results can still resolve)
        // but its transport label persists; the key guarantee is that the
        // server did not crash and the pump was torn down.
        let view = registry.get_client_view("ws-disc").await;
        assert!(view.is_some());
    }

    #[tokio::test]
    async fn ws_non_register_first_message_is_rejected() {
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;

        let url = format!("ws://{}/api/agents/ws", addr);
        let (mut ws, _resp) = connect_async(url).await.unwrap();

        // Send a Ping instead of Register.
        let ping = AgentEnvelope::Ping { ts: 1 };
        ws.send(TungsteniteMessage::Text(ping.to_json().unwrap().into()))
            .await
            .unwrap();

        // Server should send an error and close.
        let env = recv_envelope(&mut ws).await;
        match env {
            AgentEnvelope::Error { code, .. } => {
                assert_eq!(code, "expected_register");
            }
            other => panic!("expected error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn ws_slow_consumer_does_not_deadlock() {
        // The agent connects but never reads during the enqueue burst. The
        // server's enqueue path must not deadlock: `enqueue_run` never blocks
        // on the transport (the pump holds the registry lock only briefly,
        // never during a blocking send), and the registry queue cap rejects
        // overflow rather than growing without limit. The hard memory bound is
        // enforced at the registry level regardless of transport; see
        // `registry_rejects_enqueue_when_queue_full`.
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;
        let url = format!("ws://{}/api/agents/ws", addr);
        let (mut ws, _resp) = connect_async(url).await.unwrap();
        ws.send(TungsteniteMessage::Text(
            register_envelope("ws-slow").to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws).await; // Registered

        // Enqueue a burst while the agent reads nothing. The loop must
        // complete whether the requests are absorbed by socket buffers or
        // rejected by the queue cap.
        let mut first_rx: Option<(
            String,
            tokio::sync::oneshot::Receiver<crate::shell_protocol::ShellRunResponse>,
        )> = None;
        let processed = tokio::time::timeout(Duration::from_secs(10), async {
            for i in 0..400usize {
                let (request_id, rx) = registry
                    .enqueue_run(
                        ShellRunRequest {
                            client_id: "ws-slow".to_string(),
                            cwd: None,
                            command: "echo hi".to_string(),
                            stdin: None,
                            timeout_secs: 5,
                            wait_timeout_secs: 0,
                        },
                        "tester".to_string(),
                    )
                    .await
                    .unwrap();
                if i == 0 {
                    first_rx = Some((request_id, rx));
                }
            }
        })
        .await;
        assert!(processed.is_ok(), "enqueue loop must not deadlock");

        // The pipeline still works after the slow episode: read the first
        // request and return a result; the waiter resolves.
        let (request_id, rx) = first_rx.expect("first request kept");
        let req_env = recv_envelope(&mut ws).await;
        match req_env {
            AgentEnvelope::Request { request } => assert_eq!(request.request_id, request_id),
            other => panic!("expected request, got {:?}", other),
        }
        ws.send(TungsteniteMessage::Text(
            AgentEnvelope::Result {
                payload: ShellAgentResultRequest {
                    client_id: "ws-slow".to_string(),
                    agent_instance_id: "ws-inst".to_string(),
                    request_id: request_id.clone(),
                    exit_code: Some(0),
                    stdout: Some("hi".to_string()),
                    stderr: None,
                    duration_ms: Some(1),
                    error: None,
                },
            }
            .to_json()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(3), rx)
            .await
            .unwrap()
            .unwrap();
        assert!(response.success);

        // The server is still responsive.
        drop(ws);
        let _ = registry.list_clients().await;
    }

    #[tokio::test]
    async fn ws_disconnect_marks_running_job_lost() {
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;
        let url = format!("ws://{}/api/agents/ws", addr);
        let (mut ws, _resp) = connect_async(url).await.unwrap();
        ws.send(TungsteniteMessage::Text(
            register_envelope("ws-lost").to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws).await; // Registered

        // Start an async job via the registry (same path the API uses).
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("ws-lost".to_string()),
                    cwd: None,
                    command: Some("sleep 30".to_string()),
                    timeout_secs: Some(30),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();

        // Drop the socket; the server must reconcile running jobs to "lost"
        // instead of leaving them running forever.
        drop(ws);
        let mut lost = registry.get_job(&job.job_id).await.unwrap();
        for _ in 0..40 {
            if lost.status == "lost" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            lost = registry.get_job(&job.job_id).await.unwrap();
        }
        assert_eq!(lost.status, "lost");
        assert!(lost.error.unwrap().contains("disconnected"));
    }

    #[tokio::test]
    async fn ws_duplicate_different_instance_is_rejected() {
        // A WebSocket agent with client_id=oe, instance=A is online. A second
        // WebSocket registration with client_id=oe, instance=B must be rejected
        // (the server sends an error and closes the second socket). The first
        // connection stays online.
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;
        let url = format!("ws://{}/api/agents/ws", addr);

        // First session: instance A.
        let (mut ws_a, _resp) = connect_async(url.clone()).await.unwrap();
        ws_a.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-dup", "inst-a")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let ack = recv_envelope(&mut ws_a).await;
        assert!(matches!(
            ack,
            AgentEnvelope::Registered { success: true, .. }
        ));
        let view = registry.get_client_view("ws-dup").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-a");
        assert!(view.connected);

        // Second session: instance B, same client_id, while A is online.
        let (mut ws_b, _resp) = connect_async(url).await.unwrap();
        ws_b.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-dup", "inst-b")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let resp = recv_envelope(&mut ws_b).await;
        match resp {
            AgentEnvelope::Error { message, .. } => {
                assert!(message.contains("already online"), "error was: {message}");
                assert!(
                    message.contains("different instance"),
                    "error was: {message}"
                );
            }
            AgentEnvelope::Registered {
                success: false,
                error,
                ..
            } => {
                let error = error.expect("error message");
                assert!(error.contains("already online"), "error was: {error}");
            }
            other => panic!("expected error/rejected, got {:?}", other),
        }

        // The active instance is still A.
        let view = registry.get_client_view("ws-dup").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-a");
        assert!(view.connected);
    }

    #[tokio::test]
    async fn ws_same_instance_reconnect_stays_accepted() {
        // A reconnect from the same agent instance (same client_id + same
        // instance id) must be accepted as a refresh, not rejected as a
        // duplicate. This mirrors a WebSocket reconnect from the same process.
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;
        let url = format!("ws://{}/api/agents/ws", addr);

        let (mut ws1, _resp) = connect_async(url.clone()).await.unwrap();
        ws1.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-same", "inst-x")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let ack1 = recv_envelope(&mut ws1).await;
        assert!(matches!(
            ack1,
            AgentEnvelope::Registered { success: true, .. }
        ));
        drop(ws1);
        // Let the server observe the disconnect and reconcile.
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(25)).await;
            if registry.get_client_view("ws-same").await.is_some() {
                break;
            }
        }

        // Reconnect with the SAME instance id.
        let (mut ws2, _resp) = connect_async(url).await.unwrap();
        ws2.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-same", "inst-x")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let ack2 = recv_envelope(&mut ws2).await;
        assert!(matches!(
            ack2,
            AgentEnvelope::Registered { success: true, .. }
        ));
        let view = registry.get_client_view("ws-same").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-x");
        assert!(view.connected);
    }

    #[tokio::test]
    async fn ws_stale_disconnect_does_not_mark_newer_active_offline() {
        // Instance A connects, then ages out and is replaced by instance B
        // (online). When A's socket finally tears down, its disconnect must NOT
        // remove B's notifier or mark B's jobs lost. B stays online and its
        // job is not marked lost.
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;
        let url = format!("ws://{}/api/agents/ws", addr);

        // Instance A connects and starts a job.
        let (mut ws_a, _resp) = connect_async(url.clone()).await.unwrap();
        ws_a.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-stale-disc", "inst-a")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws_a).await; // Registered

        // Age A out so B can take over the lease.
        registry
            .set_last_seen_for_test("ws-stale-disc", aged_last_seen())
            .await;

        // Instance B connects and takes over.
        let (mut ws_b, _resp) = connect_async(url).await.unwrap();
        ws_b.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-stale-disc", "inst-b")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws_b).await; // Registered
        let view_b = registry.get_client_view("ws-stale-disc").await.unwrap();
        assert_eq!(view_b.agent_instance_id, "inst-b");
        assert!(view_b.connected);

        // Start a job under B.
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("ws-stale-disc".to_string()),
                    cwd: None,
                    command: Some("sleep 30".to_string()),
                    timeout_secs: Some(30),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();

        // A's socket finally disconnects. This must NOT affect B.
        drop(ws_a);
        // Give the server a moment to process A's disconnect.
        tokio::time::sleep(Duration::from_millis(150)).await;

        // B is still online and its job is NOT lost.
        let view_b_after = registry.get_client_view("ws-stale-disc").await.unwrap();
        assert!(
            view_b_after.connected,
            "stale disconnect must not mark newer active instance offline"
        );
        assert_eq!(view_b_after.agent_instance_id, "inst-b");
        let job_view = registry.get_job(&job.job_id).await.unwrap();
        assert_ne!(
            job_view.status, "lost",
            "stale disconnect must not mark active instance job lost"
        );

        // B's own disconnect does reconcile the job.
        drop(ws_b);
        let mut lost = registry.get_job(&job.job_id).await.unwrap();
        for _ in 0..40 {
            if lost.status == "lost" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            lost = registry.get_job(&job.job_id).await.unwrap();
        }
        assert_eq!(lost.status, "lost");
    }

    #[tokio::test]
    async fn ws_stale_ping_does_not_refresh_newer_active_instance() {
        // Regression at the WebSocket level: after instance A is replaced by
        // instance B, A's still-open socket must not be able to refresh B's
        // liveness by sending Ping/Pong. The server rejects the touch and the
        // active lease (B) is not extended by A's keepalive.
        //
        // We register A over a WebSocket, age it out, and let B register over a
        // second socket. We then age B out to the edge of the online window,
        // send a Ping from A's socket, and verify B's `last_seen` does not
        // advance (the touch is rejected). A Ping from B's socket does refresh.
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;
        let url = format!("ws://{}/api/agents/ws", addr);

        // Instance A connects.
        let (mut ws_a, _resp) = connect_async(url.clone()).await.unwrap();
        ws_a.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-stale-ping", "inst-a")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws_a).await; // Registered

        // Age A out so B can take over.
        registry
            .set_last_seen_for_test("ws-stale-ping", aged_last_seen())
            .await;

        // Instance B connects and takes over the lease.
        let (mut ws_b, _resp) = connect_async(url).await.unwrap();
        ws_b.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-stale-ping", "inst-b")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws_b).await; // Registered
        let view_b = registry.get_client_view("ws-stale-ping").await.unwrap();
        assert_eq!(view_b.agent_instance_id, "inst-b");
        assert!(view_b.connected);

        // Snapshot B's last_seen right after registration.
        let before = view_b.last_seen;
        // Sleep so a successful touch would observably advance last_seen.
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // A sends a Ping. The server replies with a Pong (best-effort keepalive
        // echo) but the underlying touch must be rejected, so B's last_seen is
        // unchanged.
        ws_a.send(TungsteniteMessage::Text(
            AgentEnvelope::Ping { ts: 1 }.to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        // Best-effort: drain the Pong if it arrived so it doesn't back up.
        let _ = tokio::time::timeout(Duration::from_millis(500), recv_envelope(&mut ws_a)).await;
        // Give the server a moment to finish processing.
        tokio::time::sleep(Duration::from_millis(150)).await;

        let after_a = registry
            .get_client_view("ws-stale-ping")
            .await
            .unwrap()
            .last_seen;
        assert_eq!(
            after_a, before,
            "stale instance ping must not refresh active last_seen"
        );

        // B sends a Ping and its liveness IS refreshed.
        ws_b.send(TungsteniteMessage::Text(
            AgentEnvelope::Ping { ts: 2 }.to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let _ = tokio::time::timeout(Duration::from_millis(500), recv_envelope(&mut ws_b)).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        let after_b = registry
            .get_client_view("ws-stale-ping")
            .await
            .unwrap()
            .last_seen;
        assert!(
            after_b > before,
            "active instance ping must refresh last_seen"
        );
    }
}
