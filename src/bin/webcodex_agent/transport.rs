use super::config::{max_concurrent_jobs, projects_dir, AgentConfig, QuicClientConfig};
use super::projects::AgentProjectCache;
use crate::agent_init::{TRANSPORT_AUTO, TRANSPORT_POLLING, TRANSPORT_QUIC, TRANSPORT_WEBSOCKET};
use crate::shell_protocol::{
    read_quic_frame, write_quic_frame, AgentEnvelope, QuicFrameError, ShellAgentJobUpdateRequest,
    ShellAgentJobUpdateResponse, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellAgentResultResponse, AGENT_PROTOCOL_VERSION_QUIC_V1, AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
};
use crate::{
    build_register_request, dispatch_request, handle_one_poll, register, CommandResult, JobManager,
};
use reqwest::blocking::Client;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

/// WebSocket outgoing envelope channel capacity.
pub(crate) const WS_OUTGOING_CAPACITY: usize = 64;
/// WebSocket ping interval.
const WS_PING_INTERVAL: Duration = Duration::from_secs(30);
/// Reconnect backoff after a WebSocket session ends.
const WS_RECONNECT_BACKOFF: Duration = Duration::from_secs(2);
/// Bounded wait for the writer task to flush its last frame and close the
/// sink during shutdown. A split WebSocket sink's `close()` waits for the
/// peer's close acknowledgement, which is delivered through the read half;
/// once the read loop has broken the read half is no longer polled, so
/// `close()` can hang indefinitely on a half-closed socket. Bounding it
/// guarantees `websocket_session` (and therefore the reconnect loop) always
/// makes progress instead of stalling forever after a disconnect.
const WS_WRITER_CLOSE_TIMEOUT: Duration = Duration::from_secs(1);

#[cfg(unix)]
async fn shutdown_signal() {
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = async {
            if let Some(signal) = sigterm.as_mut() {
                let _ = signal.recv().await;
            } else {
                std::future::pending::<()>().await;
            }
        } => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

/// Minimal HTTP send configuration used by the polling `AgentSink`. We do not
/// store the whole `AgentConfig` here: policy and concurrency limits stay
/// with the agent config and are passed alongside the sink.
#[derive(Debug, Clone)]
pub(crate) struct HttpSendConfig {
    pub(crate) client: Client,
    pub(crate) server_url: String,
    pub(crate) token: String,
    pub(crate) client_id: String,
    pub(crate) agent_instance_id: String,
}

/// Transport-neutral outgoing channel for an agent. Both the polling loop and
/// the WebSocket loop build an `AgentSink` and hand it to the shared
/// `dispatch_request` / `JobManager` execution path. This is the single seam
/// that lets the agent speak either transport without duplicating execution
/// logic.
#[derive(Debug, Clone)]
pub(crate) enum AgentSink {
    /// Polling transport: POST results/job_updates to the HTTP endpoints.
    Http(HttpSendConfig),
    /// WebSocket transport: push envelopes through an mpsc that a writer task
    /// drains onto the socket.
    WebSocket {
        tx: tokio::sync::mpsc::Sender<AgentEnvelope>,
        client_id: String,
        agent_instance_id: String,
    },
    /// QUIC transport: push envelopes through an mpsc that a single writer
    /// task drains onto the bidirectional stream.
    Quic {
        tx: tokio::sync::mpsc::Sender<AgentEnvelope>,
        client_id: String,
        agent_instance_id: String,
    },
}

impl AgentSink {
    pub(crate) fn client_id(&self) -> &str {
        match self {
            AgentSink::Http(h) => &h.client_id,
            AgentSink::WebSocket { client_id, .. } => client_id,
            AgentSink::Quic { client_id, .. } => client_id,
        }
    }

    /// Active agent process identity carried by this sink so every result /
    /// job_update submission includes it.
    pub(crate) fn agent_instance_id(&self) -> &str {
        match self {
            AgentSink::Http(h) => &h.agent_instance_id,
            AgentSink::WebSocket {
                agent_instance_id, ..
            } => agent_instance_id,
            AgentSink::Quic {
                agent_instance_id, ..
            } => agent_instance_id,
        }
    }

    /// Submit the result of a synchronous shell/file request. Mirrors the old
    /// `submit_result` free function but routes over the active transport.
    pub(crate) fn submit_result(
        &self,
        request_id: String,
        result: CommandResult,
    ) -> Result<bool, String> {
        let body = ShellAgentResultRequest {
            client_id: self.client_id().to_string(),
            agent_instance_id: self.agent_instance_id().to_string(),
            request_id,
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            duration_ms: result.duration_ms,
            error: result.error,
        };
        match self {
            AgentSink::Http(h) => {
                let resp: ShellAgentResultResponse = post_json_raw(
                    &h.client,
                    &h.server_url,
                    &h.token,
                    "/api/shell/agent/result",
                    &body,
                )?;
                if resp.success {
                    Ok(true)
                } else {
                    Err(resp
                        .error
                        .unwrap_or_else(|| "result submission failed without error".to_string()))
                }
            }
            AgentSink::WebSocket { tx, .. } | AgentSink::Quic { tx, .. } => {
                let env = AgentEnvelope::Result { payload: body };
                tx.blocking_send(env)
                    .map_err(|_| "agent transport send failed".to_string())?;
                Ok(true)
            }
        }
    }

    /// Push an incremental/final job update. Mirrors the old `send_job_update`
    /// free function.
    pub(crate) fn send_job_update(&self, body: &ShellAgentJobUpdateRequest) -> Result<(), String> {
        match self {
            AgentSink::Http(h) => {
                let resp: ShellAgentJobUpdateResponse = post_json_raw(
                    &h.client,
                    &h.server_url,
                    &h.token,
                    "/api/shell/agent/job_update",
                    body,
                )?;
                if resp.success {
                    Ok(())
                } else {
                    Err(resp
                        .error
                        .unwrap_or_else(|| "job_update failed without error".to_string()))
                }
            }
            AgentSink::WebSocket { tx, .. } | AgentSink::Quic { tx, .. } => {
                let env = AgentEnvelope::JobUpdate {
                    payload: body.clone(),
                };
                tx.blocking_send(env)
                    .map_err(|_| "agent transport send failed".to_string())
            }
        }
    }
}

/// Send a JSON POST to the server and decode the response. Same wire behavior
/// as `post_json` but takes the raw connection bits so it can be used from
/// `AgentSink::Http` without an `AgentConfig`.
fn post_json_raw<T, R>(
    client: &Client,
    server_url: &str,
    token: &str,
    path: &str,
    body: &T,
) -> Result<R, String>
where
    T: serde::Serialize + ?Sized,
    R: serde::de::DeserializeOwned,
{
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    let mut req = client.post(url);
    if !token.trim().is_empty() {
        req = req.bearer_auth(token.trim());
    }
    let resp = req
        .json(body)
        .send()
        .map_err(|e| format!("request {} failed: {}", path, e))?;
    let status = resp.status();
    let text = resp
        .text()
        .map_err(|e| format!("failed to read response {}: {}", path, e))?;
    if !status.is_success() {
        return Err(format!("{} returned {}: {}", path, status, text));
    }
    serde_json::from_str(&text).map_err(|e| format!("failed to parse response {}: {}", path, e))
}

pub(crate) fn non_empty_token(token: &str) -> Option<String> {
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

pub(crate) fn run_agent(cfg: AgentConfig, once: bool) -> Result<(), String> {
    // Generate the per-process agent instance identity once. It is stable for
    // the whole process lifetime, including across WebSocket reconnects, so the
    // server can treat this process as a single active lease for `client_id`.
    // It is not a secret and is never persisted to disk.
    let agent_instance_id = uuid::Uuid::new_v4().to_string();
    let transport = cfg
        .transport
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(TRANSPORT_WEBSOCKET)
        .to_string();
    match transport.as_str() {
        TRANSPORT_WEBSOCKET => run_websocket_agent(cfg, once, &agent_instance_id),
        TRANSPORT_QUIC => run_quic_agent(cfg, once, &agent_instance_id),
        TRANSPORT_AUTO => run_auto_agent(cfg, once, &agent_instance_id),
        _ => run_polling_agent(cfg, once, &agent_instance_id),
    }
}

pub(crate) fn effective_transport(cfg: &AgentConfig) -> &str {
    cfg.transport
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(TRANSPORT_WEBSOCKET)
}

pub(crate) fn auto_transport_plan(cfg: &AgentConfig) -> Vec<&'static str> {
    let mut plan = Vec::new();
    if cfg.quic.is_some() {
        plan.push(TRANSPORT_QUIC);
    }
    plan.push(TRANSPORT_WEBSOCKET);
    plan.push(TRANSPORT_POLLING);
    plan
}

fn run_auto_agent(cfg: AgentConfig, once: bool, agent_instance_id: &str) -> Result<(), String> {
    for transport in auto_transport_plan(&cfg) {
        match transport {
            TRANSPORT_QUIC => {
                eprintln!("webcodex-agent transport auto: trying quic");
                match run_quic_agent_single_session(&cfg, once, agent_instance_id) {
                    Ok(AgentSessionExit::Shutdown) => return Ok(()),
                    Ok(AgentSessionExit::Ended) if once => return Ok(()),
                    Ok(AgentSessionExit::Ended) => {
                        eprintln!(
                            "webcodex-agent transport auto: quic session ended; trying websocket"
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "webcodex-agent transport auto: quic failed: {}; trying websocket",
                            e
                        );
                    }
                }
            }
            TRANSPORT_WEBSOCKET => {
                if cfg.quic.is_none() {
                    eprintln!(
                        "webcodex-agent transport auto: [quic] not configured; trying websocket"
                    );
                } else {
                    eprintln!("webcodex-agent transport auto: trying websocket");
                }
                match run_websocket_agent_single_session(&cfg, agent_instance_id) {
                    Ok(AgentSessionExit::Shutdown) => return Ok(()),
                    Ok(AgentSessionExit::Ended) if once => return Ok(()),
                    Ok(AgentSessionExit::Ended) => {
                        eprintln!(
                            "webcodex-agent transport auto: websocket session ended; falling back to polling"
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "webcodex-agent transport auto: websocket failed: {}; falling back to polling",
                            e
                        );
                    }
                }
            }
            TRANSPORT_POLLING => {
                eprintln!("webcodex-agent transport auto: trying polling");
                return run_polling_agent(cfg, once, agent_instance_id);
            }
            _ => {}
        }
    }
    unreachable!("auto transport plan always ends with polling")
}

fn run_polling_agent(cfg: AgentConfig, once: bool, agent_instance_id: &str) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed to create http client: {}", e))?;
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let mut project_cache = AgentProjectCache::default();
    register(
        &client,
        &cfg,
        &mut project_cache,
        agent_instance_id,
        jobs.prepared_profiles.len(),
    )?;
    eprintln!(
        "webcodex-agent registered client_id={} server={} preferred_transport={} actual_transport=polling transport=polling",
        cfg.client_id,
        cfg.server_url,
        effective_transport(&cfg)
    );
    loop {
        match handle_one_poll(&client, &cfg, &jobs, &mut project_cache, agent_instance_id) {
            Ok(ran_request) => {
                if once {
                    while jobs.has_work() {
                        std::thread::sleep(Duration::from_millis(cfg.poll_interval_ms));
                    }
                    return Ok(());
                }
                if !ran_request {
                    std::thread::sleep(Duration::from_millis(cfg.poll_interval_ms));
                }
            }
            Err(e) => {
                eprintln!("webcodex-agent poll error: {}", e);
                if once {
                    return Err(e);
                }
                std::thread::sleep(Duration::from_millis(cfg.poll_interval_ms));
                let _ = register(
                    &client,
                    &cfg,
                    &mut project_cache,
                    agent_instance_id,
                    jobs.prepared_profiles.len(),
                );
            }
        }
    }
}

// ============================================================================
// Experimental custom QUIC agent transport
// ============================================================================
//
// A custom QUIC *stream* transport (NOT HTTP/3). The agent opens a single QUIC
// bidirectional stream to the server, sends a `Register` envelope carrying the
// agent token in `auth_token` (there is no HTTP middleware to set an
// `Authorization` header), reads a `Registered` ack, then handles `Request`,
// `Result`, `JobUpdate`, `Ping`, and `Pong` envelopes on that same serialized
// stream. WebSocket/polling behavior is unchanged.

/// Reconnect backoff after a QUIC session ends.
const QUIC_RECONNECT_BACKOFF: Duration = Duration::from_secs(2);
/// Interval between agent-initiated keepalive Pings.
const QUIC_PING_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentSessionExit {
    Ended,
    Shutdown,
}

/// Entry point for the QUIC transport. Runs a tokio current-thread runtime and
/// reconnects on session failure, mirroring `run_websocket_agent`.
fn run_quic_agent(cfg: AgentConfig, once: bool, agent_instance_id: &str) -> Result<(), String> {
    let agent_instance_id = agent_instance_id.to_string();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {}", e))?;
    rt.block_on(async move {
        let mut project_cache = AgentProjectCache::default();
        loop {
            let projects = project_cache.get(&cfg);
            match quic_session(&cfg, projects, &agent_instance_id, once).await {
                Ok(AgentSessionExit::Shutdown) => {
                    project_cache.invalidate();
                    eprintln!("webcodex-agent quic shutdown complete");
                    return Ok(());
                }
                Ok(AgentSessionExit::Ended) => {
                    project_cache.invalidate();
                    if once {
                        return Ok(());
                    }
                    eprintln!("webcodex-agent quic session ended; reconnecting");
                    tokio::time::sleep(QUIC_RECONNECT_BACKOFF).await;
                }
                Err(e) => {
                    project_cache.invalidate();
                    eprintln!("webcodex-agent quic error: {}", e);
                    if once {
                        return Err(e);
                    }
                    tokio::time::sleep(QUIC_RECONNECT_BACKOFF).await;
                }
            }
        }
    })
}

fn run_quic_agent_single_session(
    cfg: &AgentConfig,
    once: bool,
    agent_instance_id: &str,
) -> Result<AgentSessionExit, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {}", e))?;
    rt.block_on(async move {
        let mut project_cache = AgentProjectCache::default();
        let projects = project_cache.get(cfg);
        quic_session(cfg, projects, agent_instance_id, once).await
    })
}

/// Validate the `[quic]` config section. Returns a cloned, resolved config so
/// the session owns a concrete value (defaults applied).
pub(crate) fn resolve_quic_config(cfg: &AgentConfig) -> Result<QuicClientConfig, String> {
    let quic = cfg
        .quic
        .clone()
        .ok_or_else(|| "transport=quic requires a [quic] section in agent.toml".to_string())?;
    if quic.server_addr.trim().is_empty() {
        return Err("[quic] server_addr is required for transport=quic".to_string());
    }
    if quic.server_name.trim().is_empty() {
        return Err("[quic] server_name is required for transport=quic".to_string());
    }
    if quic.alpn.trim().is_empty() {
        return Err("[quic] alpn cannot be empty".to_string());
    }
    if quic.connect_timeout_secs == 0 {
        return Err("[quic] connect_timeout_secs must be > 0".to_string());
    }
    if quic.keepalive_interval_secs == 0 {
        return Err("[quic] keepalive_interval_secs must be > 0".to_string());
    }
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
fn rustls_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

/// Build the quinn-wrapped rustls client config for the QUIC transport. The
/// agent validates the server certificate against the Mozilla root store
/// (webpki-roots) using `server_name` as the SNI/verification name — TLS is
/// transport security, not authentication; the agent token still authenticates
/// the agent.
fn build_quic_client_crypto(
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

fn classify_quic_agent_connect_error(error: &str) -> &'static str {
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
    } else if lower.contains("alpn")
        || lower.contains("no application protocol")
        || lower.contains("applicationclosed")
        || lower.contains("connectionclosed")
        || lower.contains("closed")
    {
        "handshake failed; check WEBCODEX_QUIC_ENABLED, listener bind, and ALPN"
    } else {
        "handshake failed"
    }
}

/// One QUIC connection lifecycle: connect, register, dispatch requests until
/// the stream closes or a fatal server error arrives. In `--once` mode,
/// completes one ping/pong after the ack then returns.
async fn quic_session(
    cfg: &AgentConfig,
    projects: Vec<ShellAgentProjectSummary>,
    agent_instance_id: &str,
    once: bool,
) -> Result<AgentSessionExit, String> {
    let quic = resolve_quic_config(cfg)?;
    let client_crypto = build_quic_client_crypto(&quic)?;
    let client_config = quinn::ClientConfig::new(Arc::new(client_crypto));
    let server_addrs = resolve_quic_server_addrs(&quic.server_addr)?;
    let mut connect_errors = Vec::new();
    let mut client_endpoint = None;
    let mut conn = None;
    for server_addr in server_addrs {
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
        match tokio::time::timeout(Duration::from_secs(quic.connect_timeout_secs), connect).await {
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
                    classify_quic_agent_connect_error(&raw),
                    raw
                ));
            }
        }
    }
    let _client_endpoint = client_endpoint.ok_or_else(|| {
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
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("failed to open quic bidirectional stream: {}", e))?;

    // Register. The token is carried in `auth_token`; the server authenticates
    // it exactly like the websocket/polling paths. It is never logged.
    let register_payload = build_register_request(
        cfg,
        projects,
        AGENT_PROTOCOL_VERSION_QUIC_V1,
        agent_instance_id,
        0,
    );
    let reg_env = AgentEnvelope::Register {
        payload: register_payload,
        auth_token: non_empty_token(&cfg.token),
    };
    write_quic_frame(&mut send, &reg_env)
        .await
        .map_err(|e| format!("failed to send quic register: {}", e))?;

    // Wait for the Registered ack.
    let ack = tokio::time::timeout(Duration::from_secs(15), read_quic_frame(&mut recv))
        .await
        .map_err(|_| "quic register ack timed out".to_string())?
        .map_err(|e| format!("failed to read quic register ack: {}", e))?;
    match ack {
        AgentEnvelope::Registered { success: true, .. } => {}
        AgentEnvelope::Registered { error, .. } => {
            return Err(format!(
                "register rejected by server: {}",
                error.unwrap_or_else(|| "no server error message".to_string())
            ));
        }
        AgentEnvelope::Error { code, message } => {
            return Err(format!(
                "server error during register {}: {}",
                code, message
            ));
        }
        other => return Err(format!("expected registered ack, got {}", other.kind())),
    }
    eprintln!(
        "webcodex-agent registered client_id={} server={} preferred_transport={} actual_transport=quic transport=quic",
        cfg.client_id,
        quic.server_addr,
        effective_transport(cfg)
    );

    if once {
        // Complete one ping/pong round trip then exit, mirroring the websocket
        // `--once` semantics.
        let ping = AgentEnvelope::Ping {
            ts: chrono::Utc::now().timestamp(),
        };
        write_quic_frame(&mut send, &ping)
            .await
            .map_err(|e| format!("quic once ping send failed: {}", e))?;
        let resp = tokio::time::timeout(Duration::from_secs(10), read_quic_frame(&mut recv))
            .await
            .map_err(|_| "quic once pong timed out".to_string())?
            .map_err(|e| format!("quic once pong read failed: {}", e))?;
        match resp {
            AgentEnvelope::Pong { .. } => {}
            other => return Err(format!("expected pong, got {}", other.kind())),
        }
        let _ = write_quic_frame(
            &mut send,
            &AgentEnvelope::Goodbye {
                reason: Some("once complete".to_string()),
            },
        )
        .await;
        let _ = send.finish();
        return Ok(AgentSessionExit::Ended);
    }

    // Split into a single writer task and a reader/dispatch loop. Outgoing
    // Result/JobUpdate/Pong/Ping envelopes all pass through the channel so no
    // two tasks write the QUIC SendStream at the same time.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
    let writer_task = tokio::spawn(async move {
        while let Some(env) = out_rx.recv().await {
            if write_quic_frame(&mut send, &env).await.is_err() {
                break;
            }
        }
        let _ = send.finish();
    });

    let sink_handle = AgentSink::Quic {
        tx: out_tx.clone(),
        client_id: cfg.client_id.clone(),
        agent_instance_id: agent_instance_id.to_string(),
    };
    let jobs = JobManager::new(max_concurrent_jobs(cfg));
    let mut ping_interval = tokio::time::interval(QUIC_PING_INTERVAL);
    ping_interval.tick().await; // skip immediate first tick
    let mut shutdown = Box::pin(shutdown_signal());
    let mut shutdown_requested = false;
    let mut session_error: Option<String> = None;

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                eprintln!("webcodex-agent quic shutdown signal received");
                shutdown_requested = true;
                break;
            }
            frame = read_quic_frame(&mut recv) => {
                let env = match frame {
                    Ok(env) => env,
                    Err(QuicFrameError::EmptyStream) => break,
                    Err(e) => {
                        session_error = Some(format!("quic stream read error: {}", e));
                        break;
                    }
                };
                match env {
                    AgentEnvelope::Request { request } => {
                        let sink_handle = sink_handle.clone();
                        let policy = cfg.policy.clone();
                        let shell = cfg.shell.clone();
                        let jobs = jobs.clone();
                        let projects_dir = projects_dir(cfg);
                        tokio::task::spawn_blocking(move || {
                            let _ = dispatch_request(
                                &sink_handle,
                                &policy,
                                &shell,
                                &jobs,
                                &projects_dir,
                                request,
                            );
                        });
                    }
                    AgentEnvelope::Ping { ts } => {
                        let _ = out_tx.send(AgentEnvelope::Pong { ts }).await;
                    }
                    AgentEnvelope::Pong { .. } => {
                        // Normal keepalive response.
                    }
                    AgentEnvelope::Registered { .. } => {
                        // Ignore a redundant ack.
                    }
                    AgentEnvelope::Error { code, message } => {
                        session_error = Some(format!("server error {}: {}", code, message));
                        break;
                    }
                    other => {
                        eprintln!(
                            "webcodex-agent quic ignoring unexpected envelope: {}",
                            other.kind()
                        );
                    }
                }
            }
            _ = ping_interval.tick() => {
                let _ = out_tx
                    .send(AgentEnvelope::Ping {
                        ts: chrono::Utc::now().timestamp(),
                    })
                    .await;
            }
        }
    }

    let _ = out_tx
        .send(AgentEnvelope::Goodbye {
            reason: Some("session ending".to_string()),
        })
        .await;
    drop(out_tx);
    let mut writer_task = writer_task;
    if tokio::time::timeout(WS_WRITER_CLOSE_TIMEOUT, &mut writer_task)
        .await
        .is_err()
    {
        writer_task.abort();
    }
    if let Some(error) = session_error {
        return Err(error);
    }
    Ok(if shutdown_requested {
        AgentSessionExit::Shutdown
    } else {
        AgentSessionExit::Ended
    })
}

// ============================================================================
// WebSocket agent transport
// ============================================================================
//
// The WebSocket mode keeps one long-lived connection to the server. The server
// pushes `Request` envelopes; the agent executes them via the same
// `dispatch_request` path the polling loop uses, and sends `Result` /
// `JobUpdate` envelopes back. Polling is unchanged and remains the fallback.

/// Convert an `http(s)://` server URL into a `ws(s)://` URL plus path.
pub(crate) fn server_url_to_ws(server_url: &str, path: &str) -> Result<String, String> {
    let base = server_url.trim_end_matches('/');
    let ws = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{}{}", rest, path)
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{}{}", rest, path)
    } else if base.starts_with("ws://") || base.starts_with("wss://") {
        format!("{}{}", base, path)
    } else {
        return Err(format!(
            "server_url must be http(s)://... for websocket transport; got {}",
            server_url
        ));
    };
    Ok(ws)
}

/// Build a WebSocket handshake request, carrying a Bearer token only when the
/// configured token is non-empty. Open-mode agents intentionally send no
/// credential so the server must have explicit anonymous mode enabled.
pub(crate) fn build_ws_request(
    ws_url: &str,
    token: &str,
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request = ws_url
        .into_client_request()
        .map_err(|e| format!("invalid websocket url: {}", e))?;
    if let Some(token) = non_empty_token(token) {
        let value = format!("Bearer {}", token);
        let header_value = tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&value)
            .map_err(|e| format!("invalid token header value: {}", e))?;
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::header::AUTHORIZATION,
            header_value,
        );
    }
    Ok(request)
}

/// Entry point for the WebSocket transport. Runs a tokio current-thread
/// runtime and reconnects on session failure.
fn run_websocket_agent(
    cfg: AgentConfig,
    once: bool,
    agent_instance_id: &str,
) -> Result<(), String> {
    let agent_instance_id = agent_instance_id.to_string();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {}", e))?;
    rt.block_on(async move {
        let mut project_cache = AgentProjectCache::default();
        loop {
            let projects = project_cache.get(&cfg);
            match websocket_session(&cfg, projects, &agent_instance_id).await {
                Ok(AgentSessionExit::Shutdown) => {
                    project_cache.invalidate();
                    eprintln!("webcodex-agent websocket shutdown complete");
                    return Ok(());
                }
                Ok(AgentSessionExit::Ended) => {
                    project_cache.invalidate();
                    if once {
                        return Ok(());
                    }
                    eprintln!("webcodex-agent websocket session ended; reconnecting");
                    tokio::time::sleep(WS_RECONNECT_BACKOFF).await;
                }
                Err(e) => {
                    project_cache.invalidate();
                    eprintln!("webcodex-agent websocket error: {}", e);
                    if once {
                        return Err(e);
                    }
                    tokio::time::sleep(WS_RECONNECT_BACKOFF).await;
                }
            }
        }
    })
}

fn run_websocket_agent_single_session(
    cfg: &AgentConfig,
    agent_instance_id: &str,
) -> Result<AgentSessionExit, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {}", e))?;
    rt.block_on(async move {
        let mut project_cache = AgentProjectCache::default();
        let projects = project_cache.get(cfg);
        websocket_session(cfg, projects, agent_instance_id).await
    })
}

/// One WebSocket connection lifecycle: connect, register, then serve requests
/// until the socket closes or a fatal server error arrives.
pub(crate) async fn websocket_session(
    cfg: &AgentConfig,
    projects: Vec<ShellAgentProjectSummary>,
    agent_instance_id: &str,
) -> Result<AgentSessionExit, String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    let ws_url = server_url_to_ws(&cfg.server_url, "/api/agents/ws")?;
    let request = build_ws_request(&ws_url, &cfg.token)?;
    let (mut ws_stream, _resp) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| format!("websocket connect failed: {}", e))?;

    // Register over the socket. The prepared-profile cache is empty at
    // registration time (snapshots are prepared lazily on first use), so
    // `prepared_cache_count` is reported as 0 here.
    let register_payload = build_register_request(
        cfg,
        projects,
        AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
        agent_instance_id,
        0,
    );
    let reg_env = AgentEnvelope::Register {
        payload: register_payload,
        auth_token: None,
    };
    let reg_json =
        serde_json::to_string(&reg_env).map_err(|e| format!("failed to encode register: {}", e))?;
    ws_stream
        .send(WsMessage::Text(reg_json.into()))
        .await
        .map_err(|e| format!("failed to send register: {}", e))?;

    // Wait for Registered ack.
    let ack_msg = ws_stream
        .next()
        .await
        .ok_or_else(|| "server closed before register ack".to_string())?
        .map_err(|e| format!("failed to read register ack: {}", e))?;
    let ack_text = ack_msg
        .into_text()
        .map_err(|_| "register ack was not text".to_string())?;
    let ack = AgentEnvelope::from_slice(ack_text.as_bytes())
        .map_err(|e| format!("register ack is not a valid envelope: {}", e))?;
    match ack {
        AgentEnvelope::Registered { success: true, .. } => {}
        AgentEnvelope::Registered { error, .. } => {
            return Err(error.unwrap_or_else(|| "register rejected".to_string()));
        }
        AgentEnvelope::Error { message, .. } => return Err(message),
        other => return Err(format!("expected registered ack, got {}", other.kind())),
    }
    eprintln!(
        "webcodex-agent registered client_id={} server={} preferred_transport={} actual_transport=websocket transport=websocket",
        cfg.client_id,
        cfg.server_url,
        effective_transport(cfg)
    );

    // Split socket into writer (drains outgoing envelopes) and reader.
    let (mut sink, mut stream) = ws_stream.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
    let writer_task = tokio::spawn(async move {
        while let Some(env) = out_rx.recv().await {
            match serde_json::to_string(&env) {
                Ok(json) => {
                    if sink.send(WsMessage::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        // The sink is dropped here. We intentionally do NOT call
        // `sink.close()`: on a split WebSocket sink the close handshake is
        // delivered through the read half, which is no longer polled once
        // the read loop breaks, so an unbounded `close().await` can hang and
        // block the reconnect loop. Dropping the sink lets the OS tear down
        // the socket; the server reconciles via its disconnect path either
        // way.
    });

    let sink_handle = AgentSink::WebSocket {
        tx: out_tx.clone(),
        client_id: cfg.client_id.clone(),
        agent_instance_id: agent_instance_id.to_string(),
    };
    let jobs = JobManager::new(max_concurrent_jobs(cfg));
    let mut ping_interval = tokio::time::interval(WS_PING_INTERVAL);
    ping_interval.tick().await; // skip immediate first tick
    let mut shutdown = Box::pin(shutdown_signal());
    let mut quit_after_session = false;

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                quit_after_session = true;
                eprintln!("webcodex-agent websocket shutdown signal received");
                break;
            }
            msg = stream.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => {
                        eprintln!("webcodex-agent websocket read error: {}", e);
                        break;
                    }
                    None => break,
                };
                if msg.is_close() {
                    break;
                }
                let text = match msg.into_text() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let env = match AgentEnvelope::from_slice(text.as_bytes()) {
                    Ok(env) => env,
                    Err(e) => {
                        eprintln!("webcodex-agent websocket malformed envelope: {}", e);
                        continue;
                    }
                };
                match env {
                    AgentEnvelope::Request { request } => {
                        let sink_handle = sink_handle.clone();
                        let policy = cfg.policy.clone();
                        let shell = cfg.shell.clone();
                        let jobs = jobs.clone();
                        let projects_dir = projects_dir(&cfg);
                        // Execution is blocking (shell/file/jobs); run it off
                        // the async runtime thread. dispatch_request sends
                        // results/updates via the shared AgentSink.
                        tokio::task::spawn_blocking(move || {
                            let _ = dispatch_request(
                                &sink_handle,
                                &policy,
                                &shell,
                                &jobs,
                                &projects_dir,
                                request,
                            );
                        });
                    }
                    AgentEnvelope::Ping { ts } => {
                        let _ = out_tx.send(AgentEnvelope::Pong { ts }).await;
                    }
                    AgentEnvelope::Pong { .. } => {
                        // Normal keepalive response from the server to our
                        // Ping. This is expected liveness traffic: do not
                        // log at info level, do not disconnect, do not treat
                        // it as an unexpected envelope. Staying silent here
                        // keeps the agent log quiet during idle periods.
                    }
                    AgentEnvelope::Error { code, message } => {
                        eprintln!(
                            "webcodex-agent websocket server error {}: {}",
                            code, message
                        );
                        break;
                    }
                    other => {
                        eprintln!(
                            "webcodex-agent websocket ignoring unexpected envelope: {}",
                            other.kind()
                        );
                    }
                }
            }
            _ = ping_interval.tick() => {
                let _ = out_tx
                    .send(AgentEnvelope::Ping {
                        ts: chrono::Utc::now().timestamp(),
                    })
                    .await;
            }
        }
    }

    // Shutdown: drop the sender so the writer stops sending, drop the read
    // half so the underlying socket can be torn down, then give the writer a
    // brief grace window to flush any in-flight frame before aborting it. We
    // must NOT await an unbounded graceful close (see the writer task note):
    // bounding here guarantees `websocket_session` (and therefore the
    // reconnect loop) always makes progress after a disconnect.
    let _ = out_tx
        .send(AgentEnvelope::Goodbye {
            reason: Some("session ending".to_string()),
        })
        .await;
    drop(out_tx);
    drop(stream);
    let mut writer_task = writer_task;
    if tokio::time::timeout(WS_WRITER_CLOSE_TIMEOUT, &mut writer_task)
        .await
        .is_err()
    {
        writer_task.abort();
    }
    // Give in-flight job threads a moment to flush final updates through the
    // (now closed) sink; they will log send errors and exit on their own.
    while jobs.has_work() {
        std::thread::sleep(Duration::from_millis(cfg.poll_interval_ms.min(1000)));
    }
    Ok(if quit_after_session {
        AgentSessionExit::Shutdown
    } else {
        AgentSessionExit::Ended
    })
}
