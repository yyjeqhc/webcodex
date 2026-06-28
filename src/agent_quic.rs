//! Server-side experimental custom QUIC agent transport (Phase 5A).
//!
//! This is a **custom QUIC stream transport** for agent connections, NOT
//! HTTP/3. It runs a separate `quinn` UDP listener in parallel with the HTTP
//! server (which keeps serving GPT Actions over TCP 443 via Nginx unchanged).
//! Nginx is not involved in QUIC.
//!
//! Phase 5A scope is deliberately minimal: a QUIC connection is established,
//! the agent sends a `Register` envelope (carrying the agent token inline via
//! `auth_token`, since there is no HTTP middleware here), the server
//! authenticates it exactly like the WebSocket/polling paths, registers the
//! client into the shared [`ShellClientRegistry`] with `transport = "quic"`,
//! downgrades execution capabilities to false, and replies with a `Registered`
//! ack. After that, `Ping`/`Pong` keepalives keep the client online. Request
//! dispatch / `job_update` / stream multiplexing are explicitly out of scope
//! for 5A.
//!
//! Authentication reuses [`crate::auth::authenticate_bearer`], which mirrors
//! `AuthMiddleware`: bootstrap when auth is disabled, the server-wide token,
//! or a Phase 2/3 API/agent token looked up by SHA-256 hash. TLS certificates
//! are NOT trusted as authentication — the agent token is always validated.

use crate::auth::{authenticate_bearer, SCOPE_AGENT_REGISTER};
use crate::config::{Config, QuicServerConfig};
use crate::shell_client::{
    effective_register_owner, enforce_register_owner, require_agent_transport_scope,
    ShellClientRegistry, TRANSPORT_QUIC,
};
use crate::shell_protocol::{
    read_quic_frame, write_quic_frame, AgentEnvelope, ShellClientCapabilities,
};
use crate::Database;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use std::time::Duration;

/// The rustls crypto provider used for the QUIC transport. The dependency tree
/// pulls *both* `aws-lc-rs` and `ring`, so rustls cannot auto-select a
/// process-level provider; we therefore pin aws-lc-rs explicitly per config
/// via `builder_with_provider`. This is thread-safe (no global install) and
/// works under parallel test execution.
fn rustls_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

/// Deadline for the agent to send its first `Register` frame after the QUIC
/// handshake. Mirrors the WebSocket `REGISTER_TIMEOUT`.
const REGISTER_TIMEOUT: Duration = Duration::from_secs(15);

fn quic_phase_5a_capabilities() -> ShellClientCapabilities {
    ShellClientCapabilities {
        shell: false,
        file_read: false,
        file_write: false,
        git: false,
        jobs: false,
        async_jobs: false,
        async_shell_jobs: false,
    }
}

/// Load a PEM cert chain from `path` into DER certificates.
fn load_certs(path: &std::path::Path) -> Result<Vec<CertificateDer<'static>>, String> {
    let file = File::open(path)
        .map_err(|e| format!("failed to open QUIC cert {}: {}", path.display(), e))?;
    let mut reader = BufReader::new(file);
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<_, _>>()
        .map_err(|e| format!("failed to parse QUIC cert {}: {}", path.display(), e))?;
    if certs.is_empty() {
        return Err(format!(
            "QUIC cert {} contains no certificates",
            path.display()
        ));
    }
    Ok(certs)
}

/// Load a PEM private key from `path`. Reads the file only to parse the key;
/// never returns or logs the key *contents* (only path/parse errors).
fn load_key(path: &std::path::Path) -> Result<PrivateKeyDer<'static>, String> {
    let file = File::open(path)
        .map_err(|e| format!("failed to open QUIC key {}: {}", path.display(), e))?;
    let mut reader = BufReader::new(file);
    let key = rustls_pemfile::private_key(&mut reader)
        .map_err(|e| format!("failed to parse QUIC key {}: {}", path.display(), e))?
        .ok_or_else(|| format!("QUIC key {} contains no private key", path.display()))?;
    Ok(key)
}

/// Build a `quinn` server crypto config from PEM cert/key paths, with the
/// given ALPN. The cert/key are read once at startup; their contents are not
/// retained beyond the rustls config. Returns the quinn-wrapped
/// `QuicServerConfig` ready for `ServerConfig::with_crypto`.
fn build_server_crypto(
    quic_cfg: &QuicServerConfig,
) -> Result<quinn::crypto::rustls::QuicServerConfig, String> {
    let certs = load_certs(&quic_cfg.cert)?;
    let key = load_key(&quic_cfg.key)?;
    let mut server_crypto = rustls::ServerConfig::builder_with_provider(rustls_provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("failed to select rustls protocol versions: {}", e))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("failed to build rustls server config: {}", e))?;
    server_crypto.alpn_protocols = vec![quic_cfg.alpn.as_bytes().to_vec()];
    quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
        .map_err(|e| format!("failed to build quinn server crypto: {}", e))
}

/// Start the QUIC agent listener. Loads cert/key, binds the UDP endpoint, and
/// runs an accept loop in the caller's task. Per-connection errors are logged
/// and the loop continues; only startup failures (bad cert, bind error) are
/// returned. Runs forever once started.
pub(crate) async fn run_quic_agent_listener(
    config: Arc<Config>,
    db: Option<Arc<Database>>,
    registry: Arc<ShellClientRegistry>,
    quic_cfg: QuicServerConfig,
) -> Result<(), String> {
    quic_cfg.validate()?;
    let server_crypto = build_server_crypto(&quic_cfg)?;
    let listen: std::net::SocketAddr = quic_cfg
        .listen
        .parse()
        .map_err(|e| format!("invalid WEBCODEX_QUIC_LISTEN '{}': {}", quic_cfg.listen, e))?;
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));
    let endpoint = quinn::Endpoint::server(server_config, listen)
        .map_err(|e| format!("failed to bind QUIC listener on {}: {}", listen, e))?;
    tracing::info!(
        "Agent QUIC listener (experimental) on UDP {} with ALPN {}",
        listen,
        quic_cfg.alpn
    );
    serve_quic_endpoint(endpoint, &quic_cfg.alpn, config, db, registry).await;
    Ok(())
}

/// Accept loop shared by the production listener and tests. Runs until the
/// endpoint is closed. Each connection is handled in its own task so a slow or
/// misbehaving agent cannot block acceptance of others.
async fn serve_quic_endpoint(
    endpoint: quinn::Endpoint,
    alpn: &str,
    config: Arc<Config>,
    db: Option<Arc<Database>>,
    registry: Arc<ShellClientRegistry>,
) {
    while let Some(incoming) = endpoint.accept().await {
        let config = config.clone();
        let db = db.clone();
        let registry = registry.clone();
        let alpn = alpn.to_string();
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => {
                    handle_quic_connection(conn, &alpn, config, db, registry).await;
                }
                Err(e) => {
                    tracing::debug!(error = ?e, "quic agent connection handshake failed");
                }
            }
        });
    }
}

/// Drive one QUIC agent connection to completion: register, ack, keepalive.
async fn handle_quic_connection(
    conn: quinn::Connection,
    alpn: &str,
    config: Arc<Config>,
    db: Option<Arc<Database>>,
    registry: Arc<ShellClientRegistry>,
) {
    // ALPN is enforced by quinn during the TLS handshake: the server crypto
    // only offers the configured `alpn`, so a connection only completes when
    // the client advertises a matching ALPN. No explicit post-handshake check
    // is needed; a mismatch fails the handshake (logged in the accept loop).
    let _ = alpn;

    // The agent opens a bidirectional stream for the register/ack/keepalive
    // exchange. Phase 5A uses a single bi stream; multiplexing is 5B.
    let (mut send, mut recv) = match conn.accept_bi().await {
        Ok(pair) => pair,
        Err(e) => {
            tracing::debug!(error = ?e, "quic agent accept_bi failed");
            return;
        }
    };

    // 1. Read the first frame within a deadline; it must be a Register.
    let register_env =
        match tokio::time::timeout(REGISTER_TIMEOUT, read_quic_frame(&mut recv)).await {
            Ok(Ok(env)) => env,
            Ok(Err(e)) => {
                send_error(&mut send, &mut recv, "expected_register", &e.to_string()).await;
                return;
            }
            Err(_) => {
                send_error(
                    &mut send,
                    &mut recv,
                    "expected_register",
                    "register timed out",
                )
                .await;
                return;
            }
        };
    let (mut register_payload, auth_token) = match register_env {
        AgentEnvelope::Register {
            payload,
            auth_token,
        } => (payload, auth_token),
        other => {
            send_error(
                &mut send,
                &mut recv,
                "expected_register",
                &format!("expected register envelope, got {}", other.kind()),
            )
            .await;
            return;
        }
    };
    let client_id = register_payload.client_id.clone();
    let agent_instance_id = register_payload.agent_instance_id.clone();

    // 2. Authenticate the agent token exactly like the HTTP/WebSocket paths.
    //    The token is dropped immediately after auth so it is never logged.
    let auth = authenticate_bearer(&config, db.as_ref(), auth_token.as_deref());
    let auth = match auth {
        Some(ctx) => ctx,
        None => {
            send_error(
                &mut send,
                &mut recv,
                "unauthorized",
                "invalid or missing agent token",
            )
            .await;
            tracing::warn!(client_id = %client_id, "quic agent register rejected: unauthorized");
            return;
        }
    };

    // 3. Enforce the same transport scope/owner boundary as the WS handler.
    if let Err(e) = require_agent_transport_scope(Some(&auth), SCOPE_AGENT_REGISTER) {
        send_error(&mut send, &mut recv, "register_forbidden", &e).await;
        return;
    }
    if let Err(e) = enforce_register_owner(
        Some(&auth),
        &register_payload.client_id,
        register_payload.owner.as_deref(),
    ) {
        send_error(&mut send, &mut recv, "register_forbidden", &e).await;
        return;
    }
    register_payload.owner =
        effective_register_owner(Some(&auth), register_payload.owner.as_deref());

    // 4. Register into the shared registry (same path as polling/ws), then
    //    flip the transport label to "quic". Phase 5A deliberately supports
    //    only register/ack/ping/pong, so do not advertise execution
    //    capabilities and do not install a push notifier. The registry enqueue
    //    paths also reject quic-v1 dispatch explicitly.
    register_payload.capabilities = Some(quic_phase_5a_capabilities());
    if let Err(e) = registry.register(register_payload).await {
        send_error(&mut send, &mut recv, "register_failed", &e).await;
        return;
    }
    if let Err(e) = registry.set_transport(&client_id, TRANSPORT_QUIC).await {
        send_error(&mut send, &mut recv, "register_failed", &e).await;
        registry
            .reconcile_disconnect(&client_id, &agent_instance_id)
            .await;
        return;
    }
    let Some(view) = registry.get_client_view(&client_id).await else {
        send_error(
            &mut send,
            &mut recv,
            "register_failed",
            "client vanished after register",
        )
        .await;
        return;
    };

    // 5. Acknowledge the register.
    let ack = AgentEnvelope::Registered {
        success: true,
        client: Some(view),
        error: None,
    };
    if let Err(e) = write_quic_frame(&mut send, &ack).await {
        tracing::debug!(
            client_id = %client_id,
            error = %e,
            "quic agent register ack send failed"
        );
        registry
            .reconcile_disconnect(&client_id, &agent_instance_id)
            .await;
        return;
    }
    tracing::info!(client_id = %client_id, "agent quic connected");

    // 6. Keepalive loop: Ping -> touch + Pong; Pong -> touch. Result/JobUpdate
    //    are not expected in 5A (no dispatch) and are logged+ignored.
    loop {
        let env = match read_quic_frame(&mut recv).await {
            Ok(env) => env,
            Err(crate::shell_protocol::QuicFrameError::EmptyStream) => break,
            Err(e) => {
                tracing::debug!(
                    client_id = %client_id,
                    error = %e,
                    "quic agent stream read ended"
                );
                break;
            }
        };
        match env {
            AgentEnvelope::Ping { ts } => {
                if let Err(e) = registry.touch_client(&client_id, &agent_instance_id).await {
                    tracing::warn!(
                        client_id = %client_id,
                        error = %e,
                        "quic ping liveness touch failed"
                    );
                }
                let pong = AgentEnvelope::Pong { ts };
                if let Err(e) = write_quic_frame(&mut send, &pong).await {
                    tracing::debug!(
                        client_id = %client_id,
                        error = %e,
                        "quic pong send failed; closing"
                    );
                    break;
                }
            }
            AgentEnvelope::Pong { .. } => {
                if let Err(e) = registry.touch_client(&client_id, &agent_instance_id).await {
                    tracing::debug!(
                        client_id = %client_id,
                        error = %e,
                        "quic pong liveness touch failed"
                    );
                }
            }
            AgentEnvelope::Register { .. } => {
                // Ignore a redundant register mid-session.
            }
            other => {
                tracing::debug!(
                    client_id = %client_id,
                    kind = other.kind(),
                    "quic agent received unexpected envelope in 5A; ignoring"
                );
            }
        }
    }

    // 7. Cleanup: reconcile disconnect so running jobs are marked lost and the
    //    client decays to stale/offline via the normal online window. Mirrors
    //    the WebSocket disconnect path.
    let _ = send.finish();
    registry
        .reconcile_disconnect(&client_id, &agent_instance_id)
        .await;
    tracing::info!(client_id = %client_id, "agent quic disconnected");
}

/// Read and discard frames until the stream ends. Used to keep a QUIC
/// connection alive long enough for the peer to receive a final `Error` frame:
/// dropping the `Connection` handle sends an abrupt `CONNECTION_CLOSE` that
/// could overtake in-flight stream data, so we drain the peer's side first.
async fn drain_quic_stream(recv: &mut quinn::RecvStream) {
    loop {
        match read_quic_frame(recv).await {
            Ok(_) => continue,
            Err(_) => return,
        }
    }
}

/// Send an `Error` envelope over the stream before tearing it down, then drain
/// the peer's stream so the connection stays alive until the error is received.
async fn send_error(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    code: &str,
    message: &str,
) {
    let env = AgentEnvelope::Error {
        code: code.to_string(),
        message: message.to_string(),
    };
    if write_quic_frame(send, &env).await.is_ok() {
        let _ = send.finish();
        // Hold the connection open while the peer reads the error frame.
        drain_quic_stream(recv).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_protocol::{
        ShellClientRegisterRequest, AGENT_PROTOCOL_VERSION_QUIC_V1,
        AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
    };
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

    /// ALPN used by the QUIC integration tests.
    const TEST_ALPN: &str = "webcodex-agent/1";

    /// Generate a self-signed cert/key for `localhost` using rcgen, returned as
    /// DER types directly consumable by rustls. Avoids PEM parsing in tests.
    fn self_signed_cert() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
        let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("rcgen self-signed cert");
        let cert_der = ck.cert.der().clone();
        // rcgen serializes the key as PKCS#8 DER.
        let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(ck.key_pair.serialize_der()));
        (cert_der, key_der)
    }

    /// Build a quinn-wrapped rustls server config from the self-signed cert.
    fn server_crypto(
        cert_der: CertificateDer<'static>,
        key_der: PrivateKeyDer<'static>,
    ) -> quinn::crypto::rustls::QuicServerConfig {
        let mut cfg = rustls::ServerConfig::builder_with_provider(rustls_provider())
            .with_safe_default_protocol_versions()
            .expect("safe default protocol versions")
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .expect("rustls server config");
        cfg.alpn_protocols = vec![TEST_ALPN.as_bytes().to_vec()];
        quinn::crypto::rustls::QuicServerConfig::try_from(cfg).expect("quinn server crypto")
    }

    /// Build a quinn-wrapped rustls client config that trusts the self-signed cert.
    fn client_crypto(
        cert_der: &CertificateDer<'static>,
    ) -> quinn::crypto::rustls::QuicClientConfig {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert_der.clone()).expect("add root cert");
        let mut cfg = rustls::ClientConfig::builder_with_provider(rustls_provider())
            .with_safe_default_protocol_versions()
            .expect("safe default protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        cfg.alpn_protocols = vec![TEST_ALPN.as_bytes().to_vec()];
        quinn::crypto::rustls::QuicClientConfig::try_from(cfg).expect("quinn client crypto")
    }

    fn register_envelope(client_id: &str, instance: &str) -> AgentEnvelope {
        AgentEnvelope::Register {
            payload: ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: instance.to_string(),
                display_name: Some("quic-test".to_string()),
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
                agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string()),
                policy: None,
            },
            // Auth disabled in the test Config below -> bootstrap, token unused.
            auth_token: None,
        }
    }

    /// Bind a QUIC server endpoint on 127.0.0.1:0 and return (endpoint, addr).
    fn bind_server(
        server_crypto: quinn::crypto::rustls::QuicServerConfig,
    ) -> (quinn::Endpoint, std::net::SocketAddr) {
        let server_config = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));
        let endpoint = quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap())
            .expect("bind quic server");
        let addr = endpoint.local_addr().expect("local_addr");
        (endpoint, addr)
    }

    #[tokio::test]
    async fn quic_register_ack_and_ping_pong_roundtrip() {
        let (cert_der, key_der) = self_signed_cert();
        let server_crypto = server_crypto(cert_der.clone(), key_der);
        let (endpoint, addr) = bind_server(server_crypto);

        // Auth disabled -> bootstrap, so no token is required. This mirrors
        // the WebSocket integration tests which run without AuthMiddleware.
        let config = Arc::new(Config {
            addr: "0.0.0.0:8080".to_string(),
            data_dir: std::path::PathBuf::from("./data"),
            token: None,
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
        });
        let registry = Arc::new(ShellClientRegistry::default());

        // Spawn the accept loop.
        let serve_registry = registry.clone();
        let serve_config = config.clone();
        tokio::spawn(async move {
            serve_quic_endpoint(endpoint, TEST_ALPN, serve_config, None, serve_registry).await;
        });

        // Client: connect, open bi stream, register.
        let client_endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let conn = client_endpoint
            .connect_with(
                quinn::ClientConfig::new(Arc::new(client_crypto(&cert_der))),
                addr,
                "localhost",
            )
            .unwrap()
            .await
            .expect("quic connect");
        let (mut send, mut recv) = conn.open_bi().await.expect("open_bi");

        write_quic_frame(&mut send, &register_envelope("quic-rt", "inst-rt"))
            .await
            .expect("write register");

        // Read the Registered ack.
        let ack = tokio::time::timeout(Duration::from_secs(5), read_quic_frame(&mut recv))
            .await
            .expect("ack timeout")
            .expect("read ack");
        match ack {
            AgentEnvelope::Registered {
                success, client, ..
            } => {
                assert!(success, "register should succeed");
                let client = client.expect("client view in ack");
                assert_eq!(client.client_id, "quic-rt");
                assert_eq!(client.transport, "quic");
                assert_eq!(
                    client.agent_protocol_version,
                    AGENT_PROTOCOL_VERSION_QUIC_V1
                );
                assert!(!client.capabilities.shell);
                assert!(!client.capabilities.file_read);
                assert!(!client.capabilities.file_write);
                assert!(!client.capabilities.git);
                assert!(!client.capabilities.jobs);
                assert!(!client.capabilities.async_jobs);
                assert!(!client.capabilities.async_shell_jobs);
            }
            other => panic!("expected registered ack, got {:?}", other.kind()),
        }

        // The registry shows the agent online over QUIC.
        let view = registry
            .get_client_view("quic-rt")
            .await
            .expect("client view");
        assert!(view.connected);
        assert_eq!(view.status, "online");
        assert_eq!(view.transport, "quic");
        assert_eq!(view.agent_protocol_version, AGENT_PROTOCOL_VERSION_QUIC_V1);
        assert!(!view.capabilities.shell);
        assert!(!view.capabilities.file_read);
        assert!(!view.capabilities.file_write);
        assert!(!view.capabilities.git);
        assert!(!view.capabilities.jobs);
        assert!(!view.capabilities.async_jobs);
        assert!(!view.capabilities.async_shell_jobs);

        // Ping -> Pong, and liveness is refreshed.
        let before = view.last_seen;
        tokio::time::sleep(Duration::from_millis(1100)).await;
        write_quic_frame(&mut send, &AgentEnvelope::Ping { ts: 7 })
            .await
            .expect("write ping");
        let pong = tokio::time::timeout(Duration::from_secs(5), read_quic_frame(&mut recv))
            .await
            .expect("pong timeout")
            .expect("read pong");
        assert!(matches!(pong, AgentEnvelope::Pong { ts: 7 }));
        let after = registry
            .get_client_view("quic-rt")
            .await
            .expect("client view")
            .last_seen;
        assert!(after > before, "ping must refresh last_seen");

        // Close the stream; the server reconciles.
        send.finish().unwrap();
        // Give the server a moment to observe the stream end.
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(25)).await;
            if registry.get_client_view("quic-rt").await.is_some() {
                break;
            }
        }
        client_endpoint.close(quinn::VarInt::from_u32(0), b"");
        conn.close(quinn::VarInt::from_u32(0), b"done");
    }

    #[tokio::test]
    async fn quic_non_register_first_frame_is_rejected() {
        let (cert_der, key_der) = self_signed_cert();
        let server_crypto = server_crypto(cert_der.clone(), key_der);
        let (endpoint, addr) = bind_server(server_crypto);
        let config = Arc::new(Config {
            addr: "0.0.0.0:8080".to_string(),
            data_dir: std::path::PathBuf::from("./data"),
            token: None,
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
        });
        let registry = Arc::new(ShellClientRegistry::default());
        let serve_registry = registry.clone();
        let serve_config = config.clone();
        tokio::spawn(async move {
            serve_quic_endpoint(endpoint, TEST_ALPN, serve_config, None, serve_registry).await;
        });

        let client_endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let conn = client_endpoint
            .connect_with(
                quinn::ClientConfig::new(Arc::new(client_crypto(&cert_der))),
                addr,
                "localhost",
            )
            .unwrap()
            .await
            .expect("quic connect");
        let (mut send, mut recv) = conn.open_bi().await.expect("open_bi");

        // Send a Ping instead of Register.
        write_quic_frame(&mut send, &AgentEnvelope::Ping { ts: 1 })
            .await
            .unwrap();

        // The server sends an Error and closes the stream.
        let env = tokio::time::timeout(Duration::from_secs(5), read_quic_frame(&mut recv))
            .await
            .expect("timeout")
            .expect("read");
        match env {
            AgentEnvelope::Error { code, .. } => assert_eq!(code, "expected_register"),
            other => panic!("expected error, got {:?}", other.kind()),
        }

        // No client was registered.
        assert!(registry.get_client_view("quic-reject").await.is_none());
        assert!(registry.list_clients().await.is_empty());
        client_endpoint.close(quinn::VarInt::from_u32(0), b"");
        conn.close(quinn::VarInt::from_u32(0), b"done");
    }

    /// A QUIC-registered agent must surface the `quic-v1` protocol version and
    /// `quic` transport in `list_clients` (used by runtime_status / listAgents).
    #[tokio::test]
    async fn quic_agent_surfaces_transport_and_protocol_in_list() {
        let (cert_der, key_der) = self_signed_cert();
        let server_crypto = server_crypto(cert_der.clone(), key_der);
        let (endpoint, addr) = bind_server(server_crypto);
        let config = Arc::new(Config {
            addr: "0.0.0.0:8080".to_string(),
            data_dir: std::path::PathBuf::from("./data"),
            token: None,
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
        });
        let registry = Arc::new(ShellClientRegistry::default());
        let serve_registry = registry.clone();
        let serve_config = config.clone();
        tokio::spawn(async move {
            serve_quic_endpoint(endpoint, TEST_ALPN, serve_config, None, serve_registry).await;
        });

        let client_endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let conn = client_endpoint
            .connect_with(
                quinn::ClientConfig::new(Arc::new(client_crypto(&cert_der))),
                addr,
                "localhost",
            )
            .unwrap()
            .await
            .expect("quic connect");
        let (mut send, mut recv) = conn.open_bi().await.expect("open_bi");
        write_quic_frame(&mut send, &register_envelope("quic-list", "inst-list"))
            .await
            .unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(5), read_quic_frame(&mut recv))
            .await
            .expect("ack timeout")
            .expect("read ack");

        let clients = registry.list_clients().await;
        assert_eq!(clients.len(), 1);
        let c = &clients[0];
        assert_eq!(c.client_id, "quic-list");
        assert_eq!(c.transport, "quic");
        assert_eq!(c.agent_protocol_version, AGENT_PROTOCOL_VERSION_QUIC_V1);
        assert!(c.connected);
        assert!(!c.capabilities.shell);
        assert!(!c.capabilities.file_read);
        assert!(!c.capabilities.file_write);
        assert!(!c.capabilities.git);
        assert!(!c.capabilities.jobs);
        assert!(!c.capabilities.async_jobs);
        assert!(!c.capabilities.async_shell_jobs);

        // Ensure the websocket protocol label is distinct (sanity for the
        // status sanitization test requirement).
        assert_ne!(
            c.agent_protocol_version,
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V1
        );

        send.finish().unwrap();
        client_endpoint.close(quinn::VarInt::from_u32(0), b"");
        conn.close(quinn::VarInt::from_u32(0), b"done");
    }
}
