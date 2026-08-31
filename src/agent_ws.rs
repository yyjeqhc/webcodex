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

use crate::shell_client::{AgentTransport, ShellClientRegistry};
use crate::shell_protocol::{AgentEnvelope, ShellClientRegisterRequest};
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

/// WebSocket agent endpoint: `GET /api/agents/ws`. Requires auth via the shared
/// `AuthMiddleware`, exactly like the polling endpoints. Authentication is
/// `Authorization: Bearer <token>`; query-string credentials are not accepted.
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
    let mut register_payload = match read_register(&mut ws).await {
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
    let connection_id = uuid::Uuid::new_v4().to_string();

    // 1b. Enforce the agent transport boundary before mutating the registry.
    //     This mirrors the polling register handler: bootstrap may register
    //     any owner; an agent token may register only when its
    //     allowed_client_id matches and its owner matches the requested owner
    //     (or fills it in when absent); user tokens are rejected. When no
    //     AuthContext is present (unit tests without AuthMiddleware) the check
    //     is a no-op; production always runs behind AuthMiddleware. The shared
    //     `register_session_prelude` performs the scope/owner checks and
    //     resolves the effective owner; it stops before any wire I/O so this
    //     handler sends its own error envelope.
    if let Err(e) =
        crate::agent_session::register_session_prelude(auth.as_ref(), &mut register_payload)
    {
        send_envelope_or_log(
            &mut ws,
            AgentEnvelope::Error {
                code: crate::agent_session::RegisterPreludeError::CODE.to_string(),
                message: e.message().to_string(),
            },
            crate::agent_session::RegisterPreludeError::CODE,
        )
        .await;
        return;
    }

    // 2. Commit the complete streaming session in one registry transaction.
    //    Transport identity comes from this handler, not the raw protocol label.
    let notify = Arc::new(Notify::new());
    let (view, cancel) = match registry
        .register_streaming_session_with_cancel(
            register_payload,
            auth.as_ref(),
            &connection_id,
            AgentTransport::WebSocket,
            notify.clone(),
        )
        .await
    {
        Ok(session) => session,
        Err(e) => {
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
    };

    // 3. Acknowledge the register. A failed post-commit ack means this
    //    concrete connection never completed its handshake, so revoke only
    //    this exact connection lease before returning. A same-instance newer
    //    reconnect remains protected by the connection_id fence.
    if send_envelope(
        &mut ws,
        AgentEnvelope::Registered {
            success: true,
            client: Some(view),
            error: None,
        },
    )
    .await
    .is_err()
    {
        tracing::debug!(client_id = %client_id, "agent websocket registered ack send failed");
        registry
            .reconcile_disconnect_for_connection(&client_id, &agent_instance_id, &connection_id)
            .await;
        return;
    }
    tracing::info!(client_id = %client_id, "agent websocket connected");

    // 4. Split the socket into a writer (owned by a writer task) and a reader
    //    (owned by this task). Outgoing envelopes go through a single mpsc so
    //    the request pump and pong replies share one writer. The channel
    //    carries `AgentEnvelope`s; the writer serializes each to text, so the
    //    shared session loop (`run_agent_session`) is transport-neutral.
    let (sink, stream) = ws.split();
    let (out_tx, out_rx) =
        mpsc::channel::<AgentEnvelope>(crate::agent_session::OUTGOING_CHANNEL_CAPACITY);

    let writer_task = tokio::spawn(async move {
        let mut sink = sink;
        let mut out_rx = out_rx;
        while let Some(env) = out_rx.recv().await {
            let Ok(json) = env.to_json() else {
                return crate::agent_session::WriterExit::TransportFailed;
            };
            if sink.send(Message::text(json)).await.is_err() {
                return crate::agent_session::WriterExit::TransportFailed;
            }
        }
        if sink.close().await.is_err() {
            crate::agent_session::WriterExit::TransportFailed
        } else {
            crate::agent_session::WriterExit::ChannelClosed
        }
    });

    // 5-7. Pump, reader loop, and teardown are shared with the QUIC transport.
    //      The reader adapter translates tungstenite messages into the
    //      transport-neutral `RecvOutcome`.
    let reader = WsReader { stream };
    crate::agent_session::run_agent_session(
        crate::agent_session::SessionContext {
            registry: &registry,
            client_id: &client_id,
            agent_instance_id: &agent_instance_id,
            connection_id: &connection_id,
            notify,
            cancel,
            transport_label: "websocket",
        },
        out_tx,
        reader,
        writer_task,
    )
    .await;
    tracing::info!(client_id = %client_id, "agent websocket disconnected");
}

/// Adapter turning a tungstenite/salvo WebSocket read stream into the
/// transport-neutral [`crate::agent_session::AgentReader`].
///
/// `Ping`/`Pong`/`Binary` frames are skipped (tungstenite auto-replies to
/// protocol pings), close frames stop the reader, and malformed text envelopes
/// are logged and skipped rather than fatal.
struct WsReader {
    stream: futures_util::stream::SplitStream<WebSocket>,
}

impl crate::agent_session::AgentReader for WsReader {
    async fn recv(&mut self) -> crate::agent_session::RecvOutcome {
        use futures_util::StreamExt;
        let Some(msg) = self.stream.next().await else {
            return crate::agent_session::RecvOutcome::Closed;
        };
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(error = ?e, "agent websocket read error");
                return crate::agent_session::RecvOutcome::Closed;
            }
        };
        if msg.is_close() {
            return crate::agent_session::RecvOutcome::Closed;
        }
        // tungstenite auto-replies to Ping with Pong at the protocol level,
        // so we only react to application Text messages here.
        let text = match msg.as_str() {
            Ok(s) => s,
            Err(_) => return crate::agent_session::RecvOutcome::Skip,
        };
        match AgentEnvelope::from_slice(text.as_bytes()) {
            Ok(env) => crate::agent_session::RecvOutcome::Envelope(env),
            Err(e) => {
                tracing::debug!(error = %e, "agent websocket received malformed envelope; ignoring");
                crate::agent_session::RecvOutcome::Skip
            }
        }
    }
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
        AgentEnvelope::Register { payload, .. } => Ok(payload),
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
        AgentPolicySummary, ClaudeCodeProviderStatus, ProviderCallSummary, ShellAgentResultRequest,
        ShellClientCapabilities, ShellClientRegisterRequest, ShellJobOpRequest, ShellRunRequest,
        ToolProvidersStatus,
    };
    use salvo::conn::{Acceptor, Listener};
    use std::net::SocketAddr;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    async fn wait_for_ws_client_connected(
        registry: &ShellClientRegistry,
        client_id: &str,
        expected: bool,
    ) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let view = registry.get_client_view(client_id).await.unwrap();
            if view.connected == expected {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "client {client_id} connected={expected} was not observed before the 3-second deadline; last status={} transport={}",
                view.status,
                view.transport
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn wait_for_ws_job_status(registry: &ShellClientRegistry, job_id: &str, expected: &str) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let job = registry.get_job(job_id).await.unwrap();
            if job.status == expected {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "job {job_id} did not reach {expected} before the 3-second deadline; last status={}",
                job.status
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    fn register_envelope(client_id: &str) -> AgentEnvelope {
        register_envelope_with_instance(client_id, "ws-inst")
    }

    fn register_envelope_with_instance(client_id: &str, instance_id: &str) -> AgentEnvelope {
        AgentEnvelope::Register {
            payload: ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
                client_id: client_id.to_string(),
                agent_instance_id: instance_id.to_string(),
                display_name: Some("ws-test".to_string()),
                owner: Some("tester".to_string()),
                hostname: None,
                host_context: None,
                capabilities: Some(ShellClientCapabilities {
                    shell: true,
                    file_read: true,
                    file_write: true,
                    artifact_export_chunk_read: false,
                    artifact_export_streaming_metadata: false,
                    structured_file_delete: true,
                    apply_text_edit_occurrence: false,
                    git: false,
                    jobs: true,
                    async_jobs: true,
                    async_shell_jobs: true,
                    ssh_shell: false,
                    persistent_shell: false,
                    ssh_persistent_shell: false,
                    structured_validation_argv: true,
                    structured_cargo_test_count_assertion: true,
                    structured_go_test_json: true,
                    structured_go_test_tool: true,
                    structured_go_test_packages: true,
                    structured_process_argv: true,
                    structured_script_payload: false,
                    internal_posix_script: false,
                    structured_execution_jobs: false,
                    detached_process_jobs: false,
                    lsp_read_only_navigation: false,
                    lsp_call_hierarchy: false,
                    sandbox_inspect_commands: false,
                    project_lifecycle: false,
                    project_path_registration: false,
                    skill_store_read: false,
                    skill_store_manage: false,
                    computer_observe: false,
                    computer_application_discovery: false,
                    computer_application_launch: false,
                    computer_display_observe: false,
                    computer_pointer_control: false,
                    computer_clipboard_read: false,
                    computer_clipboard_write: false,
                    computer_snapshot_region: false,
                    computer_accessibility_observe: false,
                    computer_element_state: false,
                    computer_control: false,
                    computer_scroll_to_element: false,
                    computer_key_input: false,
                    computer_window_activate: false,
                    computer_text_input: false,
                    job_state_reconciliation: false,
                    coding_agent_runs: false,
                    agent_protocol_generation: None,
                }),
                projects: None,
                agent_protocol_version: Some(
                    crate::shell_protocol::AGENT_PROTOCOL_VERSION_WEBSOCKET_V1.to_string(),
                ),
                policy: Some(AgentPolicySummary::default()),
            },
        }
    }

    /// A `last_seen` timestamp comfortably past the 60s online window, used to
    /// simulate liveness decay without a real sleep. The window constant lives
    /// in `shell_client` and is private, so we use a generous 2-minute age.
    fn aged_last_seen() -> i64 {
        chrono::Utc::now().timestamp() - 120
    }

    fn provider_status() -> ToolProvidersStatus {
        ToolProvidersStatus {
            strategy: "claude_code".to_string(),
            claude_code: ClaudeCodeProviderStatus {
                enabled: true,
                version: Some("2.1.217".to_string()),
                available: true,
                process_state: "running".to_string(),
                discovered_tool_names: vec!["Edit".to_string()],
                capabilities: std::collections::BTreeMap::from([
                    ("edit_file".to_string(), "available".to_string()),
                    ("search_project_text".to_string(), "unmapped".to_string()),
                ]),
                last_error_code: None,
                last_call: Some(ProviderCallSummary {
                    capability: "edit_file".to_string(),
                    selected_provider: "claude_code".to_string(),
                    fallback_used: false,
                    result: "success".to_string(),
                    write_state: Some("confirmed".to_string()),
                    duration_ms: 8,
                    error_code: None,
                }),
            },
            config_reload: Default::default(),
        }
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

    async fn start_authenticated_server(
        registry: Arc<ShellClientRegistry>,
    ) -> (SocketAddr, tempfile::TempDir) {
        let config = crate::test_support::test_config(Some("bootstrap-secret"));
        let (tmp, db) = crate::test_support::test_db();
        let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
        let addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let router = Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(registry))
            .push(
                Router::with_path("api/agents/ws")
                    .hoop(crate::auth::AuthMiddleware)
                    .goal(agent_ws),
            );
        tokio::spawn(async move {
            Server::new(acceptor).serve(router).await;
        });
        (addr, tmp)
    }

    async fn connect_with_bearer(
        url: &str,
        token: &str,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let mut request = url.into_client_request().unwrap();
        request.headers_mut().insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        connect_async(request).await.expect("ws connect").0
    }

    fn shared_key_register_envelope(
        client_id: &str,
        instance_id: &str,
        project_id: &str,
    ) -> AgentEnvelope {
        let AgentEnvelope::Register { mut payload, .. } =
            register_envelope_with_instance(client_id, instance_id)
        else {
            unreachable!()
        };
        payload.owner = Some("untrusted-owner".to_string());
        payload.projects = Some(vec![crate::shell_protocol::ShellAgentProjectSummary {
            id: project_id.to_string(),
            name: Some(project_id.to_string()),
            path: format!("/tmp/{project_id}"),
            allow_patch: true,
            kind: None,
            description: None,
            hooks: Vec::new(),
            disabled: false,
            revision: None,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: chrono::Utc::now().timestamp(),
            shell_profile: None,
        }]);
        AgentEnvelope::Register { payload }
    }

    #[tokio::test]
    async fn ws_direct_shared_keys_register_by_group_and_cannot_spoof_results() {
        let env = crate::auth::AuthEnvGuard::auth_required();
        env.enable_direct_shared_key();
        let registry = Arc::new(ShellClientRegistry::default());
        let (addr, _tmp) = start_authenticated_server(registry.clone()).await;
        let url = format!("ws://{addr}/api/agents/ws");

        let mut ws_a = connect_with_bearer(&url, "shared-key-a").await;
        ws_a.send(TungsteniteMessage::Text(
            shared_key_register_envelope("shared-a", "instance-a", "project-a")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let AgentEnvelope::Registered {
            success: true,
            client: Some(client_a),
            ..
        } = recv_envelope(&mut ws_a).await
        else {
            panic!("shared key A should register")
        };
        assert_eq!(client_a.client_id, "shared-a");
        assert_eq!(client_a.owner, None, "shared-key owner must be ignored");

        let auth_a = crate::auth::shared_key::shared_key_context("shared-key-a");
        let auth_b = crate::auth::shared_key::shared_key_context("shared-key-b");
        assert!(registry
            .get_client_view_for_auth("shared-a", Some(&auth_a))
            .await
            .is_some());
        assert!(registry
            .get_client_view_for_auth("shared-a", Some(&auth_b))
            .await
            .is_none());

        let mut ws_collision = connect_with_bearer(&url, "shared-key-b").await;
        ws_collision
            .send(TungsteniteMessage::Text(
                shared_key_register_envelope("shared-a", "instance-a", "project-b")
                    .to_json()
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
        let AgentEnvelope::Error { code, message } = recv_envelope(&mut ws_collision).await else {
            panic!("cross-group client_id collision should fail")
        };
        assert_eq!(code, "register_failed");
        assert_eq!(message, "agent client identity is unavailable");

        let mut ws_b = connect_with_bearer(&url, "shared-key-b").await;
        ws_b.send(TungsteniteMessage::Text(
            shared_key_register_envelope("shared-b", "instance-b", "project-b")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        assert!(matches!(
            recv_envelope(&mut ws_b).await,
            AgentEnvelope::Registered { success: true, .. }
        ));

        let (request_id, mut result_rx) = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "shared-a".to_string(),
                    cwd: None,
                    command: "echo shared-a".to_string(),
                    stdin: None,
                    timeout_secs: 5,
                    wait_timeout_secs: 0,
                },
                "anonymous".to_string(),
            )
            .await
            .unwrap();
        assert!(matches!(
            recv_envelope(&mut ws_a).await,
            AgentEnvelope::Request { .. }
        ));

        // Key B knows the public ids but its registered connection identity
        // must not be able to submit Key A's result.
        ws_b.send(TungsteniteMessage::Text(
            AgentEnvelope::Result {
                payload: ShellAgentResultRequest {
                    client_id: "shared-a".to_string(),
                    agent_instance_id: "instance-a".to_string(),
                    request_id: request_id.clone(),
                    exit_code: Some(0),
                    stdout: Some("spoofed".to_string()),
                    stderr: None,
                    duration_ms: Some(1),
                    error: None,
                }
                .into(),
            }
            .to_json()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(150), &mut result_rx)
                .await
                .is_err(),
            "cross-group result must not resolve Key A's request"
        );

        ws_a.send(TungsteniteMessage::Text(
            AgentEnvelope::Result {
                payload: ShellAgentResultRequest {
                    client_id: "shared-a".to_string(),
                    agent_instance_id: "instance-a".to_string(),
                    request_id,
                    exit_code: Some(0),
                    stdout: Some("authentic".to_string()),
                    stderr: None,
                    duration_ms: Some(1),
                    error: None,
                }
                .into(),
            }
            .to_json()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(3), result_rx)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.stdout.as_deref(), Some("authentic"));
    }

    #[tokio::test]
    async fn ws_register_requires_explicit_protocol_version() {
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;
        let url = format!("ws://{}/api/agents/ws", addr);
        let (mut ws, _resp) = connect_async(url).await.expect("ws connect");
        let mut register = register_envelope("ws-missing-protocol");
        let AgentEnvelope::Register { payload, .. } = &mut register else {
            unreachable!("register helper must return Register")
        };
        payload.agent_protocol_version = None;
        ws.send(TungsteniteMessage::Text(register.to_json().unwrap().into()))
            .await
            .unwrap();

        match recv_envelope(&mut ws).await {
            AgentEnvelope::Error { code, message } => {
                assert_eq!(code, "register_failed");
                assert_eq!(message, "agent_protocol_version is required");
            }
            other => panic!("expected register_failed, got {:?}", other.kind()),
        }
        assert!(registry
            .get_client_view("ws-missing-protocol")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn ws_register_rejects_unsupported_protocol_version() {
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;
        let url = format!("ws://{}/api/agents/ws", addr);
        let (mut ws, _resp) = connect_async(url).await.expect("ws connect");
        let mut register = register_envelope("ws-unsupported-protocol");
        let AgentEnvelope::Register { payload, .. } = &mut register else {
            unreachable!("register helper must return Register")
        };
        payload.agent_protocol_version = Some("websocket-next".to_string());
        ws.send(TungsteniteMessage::Text(register.to_json().unwrap().into()))
            .await
            .unwrap();

        match recv_envelope(&mut ws).await {
            AgentEnvelope::Error { code, message } => {
                assert_eq!(code, "register_failed");
                assert_eq!(message, "agent_protocol_version is unsupported");
            }
            other => panic!("expected register_failed, got {:?}", other.kind()),
        }
        assert!(registry
            .get_client_view("ws-unsupported-protocol")
            .await
            .is_none());
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
            }
            .into(),
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

        ws.send(TungsteniteMessage::Text(
            AgentEnvelope::RuntimeMetadata {
                tool_providers: provider_status(),
            }
            .to_json()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
        let metadata_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let view = registry.get_client_view("ws-roundtrip").await.unwrap();
            if view
                .policy
                .as_ref()
                .and_then(|policy| policy.tool_providers.as_ref())
                .and_then(|providers| providers.claude_code.last_call.as_ref())
                .is_some()
            {
                break;
            }
            assert!(
                tokio::time::Instant::now() < metadata_deadline,
                "runtime metadata was not projected before the 3-second deadline"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let view = registry.get_client_view("ws-roundtrip").await.unwrap();
        let call = view
            .policy
            .unwrap()
            .tool_providers
            .unwrap()
            .claude_code
            .last_call
            .unwrap();
        assert_eq!(call.selected_provider, "claude_code");
        assert_eq!(call.write_state.as_deref(), Some("confirmed"));
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

        wait_for_ws_client_connected(&registry, "ws-pong", true).await;
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

        // Disconnect: server reconciles and retains the client record offline.
        drop(ws1);
        wait_for_ws_client_connected(&registry, "ws-recon", false).await;

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
                assert!(client.capabilities.structured_file_delete);
            }
            other => panic!("expected registered ack on reconnect, got {:?}", other),
        }

        let view2 = registry.get_client_view("ws-recon").await.unwrap();
        assert_eq!(view2.transport, "websocket");
        assert!(view2.connected);
        assert_eq!(view2.status, "online");
        assert!(view2.capabilities.structured_file_delete);
    }

    #[tokio::test]
    async fn ws_disconnect_marks_client_offline_and_retains_record() {
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

        drop(ws);
        wait_for_ws_client_connected(&registry, "ws-disc", false).await;
        let view = registry.get_client_view("ws-disc").await.unwrap();
        assert_eq!(view.transport, "websocket");
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
                }
                .into(),
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
        wait_for_ws_job_status(&registry, &job.job_id, "lost").await;
        let lost = registry.get_job(&job.job_id).await.unwrap();
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
        wait_for_ws_client_connected(&registry, "ws-same", false).await;

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
    async fn ws_goodbye_releases_lease_for_new_instance() {
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;
        let url = format!("ws://{}/api/agents/ws", addr);

        let (mut ws_a, _resp) = connect_async(url.clone()).await.unwrap();
        ws_a.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-goodbye", "inst-a")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        assert!(matches!(
            recv_envelope(&mut ws_a).await,
            AgentEnvelope::Registered { success: true, .. }
        ));

        ws_a.send(TungsteniteMessage::Text(
            AgentEnvelope::Goodbye {
                reason: Some("test shutdown".to_string()),
            }
            .to_json()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
        wait_for_ws_client_connected(&registry, "ws-goodbye", false).await;
        let offline = registry.get_client_view("ws-goodbye").await.unwrap();
        assert!(!offline.connected);

        let (mut ws_b, _resp) = connect_async(url).await.unwrap();
        ws_b.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-goodbye", "inst-b")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        assert!(matches!(
            recv_envelope(&mut ws_b).await,
            AgentEnvelope::Registered { success: true, .. }
        ));
        let view = registry.get_client_view("ws-goodbye").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-b");
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
        wait_for_ws_job_status(&registry, &job.job_id, "lost").await;
        let lost = registry.get_job(&job.job_id).await.unwrap();
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

        // P5a actively terminates A after B commits, so a stale socket no
        // longer remains usable as a keepalive source at all. Observe its
        // terminal state within a bounded window, then verify B's liveness was
        // not refreshed while only the replaced connection was being closed.
        let stale_exit = tokio::time::timeout(Duration::from_millis(500), ws_a.next())
            .await
            .expect("replaced WebSocket A must terminate promptly");
        assert!(
            matches!(
                stale_exit,
                None | Some(Ok(TungsteniteMessage::Close(_))) | Some(Err(_))
            ),
            "replaced WebSocket A must close instead of receiving application traffic"
        );

        let after_a = registry
            .get_client_view("ws-stale-ping")
            .await
            .unwrap()
            .last_seen;
        assert_eq!(
            after_a, before,
            "replaced instance termination must not refresh active last_seen"
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

    #[tokio::test]
    async fn ws_stale_connection_pump_cannot_steal_new_request() {
        // Same runner instance connects over socket A, then reconnects over
        // socket B (same agent_instance_id, new connection_id lease). A
        // request enqueued after B's register must be delivered to B's pump
        // only — A's pump is bound to the stale connection lease and the
        // connection-scoped poll rejects it, so A never receives the request
        // and the request is dispatched exactly once.
        let registry = Arc::new(ShellClientRegistry::default());
        let addr = start_server(registry.clone()).await;
        let url = format!("ws://{}/api/agents/ws", addr);

        // Connection A registers.
        let (mut ws_a, _resp) = connect_async(url.clone()).await.unwrap();
        ws_a.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-steal", "inst-x")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws_a).await; // Registered

        // Same instance reconnects over B (reconnect/refresh: accepted).
        let (mut ws_b, _resp) = connect_async(url).await.unwrap();
        ws_b.send(TungsteniteMessage::Text(
            register_envelope_with_instance("ws-steal", "inst-x")
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let _ = recv_envelope(&mut ws_b).await; // Registered

        // Enqueue a request after B's register: it belongs to B's lease.
        let (request_id, _rx) = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "ws-steal".to_string(),
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

        // B's pump receives the request.
        let req_env = tokio::time::timeout(Duration::from_secs(2), recv_envelope(&mut ws_b))
            .await
            .unwrap();
        match req_env {
            AgentEnvelope::Request { request } => assert_eq!(request.request_id, request_id),
            other => panic!("expected request on B, got {:?}", other),
        }

        // P5a actively cancels A once B commits. The stale socket must
        // terminate promptly; it must not remain open/quiet behind a dead pump.
        let stale_exit = tokio::time::timeout(Duration::from_millis(500), ws_a.next())
            .await
            .expect("replaced WebSocket A must terminate promptly");
        assert!(
            matches!(
                stale_exit,
                None | Some(Ok(TungsteniteMessage::Close(_))) | Some(Err(_))
            ),
            "replaced WebSocket A must close instead of receiving application traffic"
        );
    }
}
