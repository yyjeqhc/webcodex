use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::DoctorOptions;

use super::{http_post_json_status, read_env_file_value, read_optional_token, DoctorCheck};

pub(crate) fn resolve_doctor_general_token(opts: &DoctorOptions) -> Result<Option<String>, String> {
    if let Some(token) = read_optional_token(&opts.token_file, "--token-file")? {
        return Ok(Some(token));
    }
    if let Some(path) = &opts.env_file {
        if let Some(token) = read_env_file_value(path, "WEBCODEX_TOKEN")? {
            let token = token.trim().to_string();
            if !token.is_empty() {
                return Ok(Some(token));
            }
        }
    }
    Ok(None)
}

fn rustls_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

fn build_doctor_quic_client_crypto(
    alpn: &str,
) -> Result<quinn::crypto::rustls::QuicClientConfig, String> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut client_crypto = rustls::ClientConfig::builder_with_provider(rustls_provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("failed to select rustls protocol versions: {}", e))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    client_crypto.alpn_protocols = vec![alpn.as_bytes().to_vec()];
    quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
        .map_err(|e| format!("failed to build quic client crypto: {}", e))
}

#[derive(Debug, Clone)]
pub(crate) struct DoctorQuicResolved {
    pub(crate) server_addr: String,
    pub(crate) server_name: String,
    pub(crate) alpn: String,
    pub(crate) timeout_secs: u64,
    pub(crate) client_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorRuntimeQuicStatus {
    enabled: bool,
    listen: String,
    alpn: String,
    listener_started: bool,
    last_error: Option<String>,
}

fn sanitize_doctor_quic_error(error: &str) -> String {
    let compact = error.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = compact.to_ascii_lowercase();
    if lower.contains("quic_cert") && lower.contains("does not exist") {
        return "WEBCODEX_QUIC_CERT path does not exist".to_string();
    }
    if lower.contains("quic_key") && lower.contains("does not exist") {
        return "WEBCODEX_QUIC_KEY path does not exist".to_string();
    }
    if (lower.contains("quic cert") || lower.contains("quic key") || lower.contains("private key"))
        && lower.contains('/')
    {
        return "QUIC listener startup error; check runtime_status.last_error and journalctl"
            .to_string();
    }
    compact.chars().take(240).collect()
}

fn parse_runtime_quic_status(output: &Value) -> Option<DoctorRuntimeQuicStatus> {
    let quic = output.get("quic")?;
    Some(DoctorRuntimeQuicStatus {
        enabled: quic
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        listen: quic
            .get("listen")
            .and_then(Value::as_str)
            .unwrap_or("(unknown)")
            .to_string(),
        alpn: quic
            .get("alpn")
            .and_then(Value::as_str)
            .unwrap_or("(unknown)")
            .to_string(),
        listener_started: quic
            .get("listener_started")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        last_error: quic
            .get("last_error")
            .and_then(Value::as_str)
            .map(sanitize_doctor_quic_error),
    })
}

pub(crate) fn doctor_runtime_quic_checks(output: &Value) -> (Vec<DoctorCheck>, bool) {
    let Some(status) = parse_runtime_quic_status(output) else {
        return (
            vec![DoctorCheck::warn(
                "quic runtime config",
                "not exposed by this server version; check server logs",
            )],
            true,
        );
    };

    let detail = format!(
        "enabled={} listen={} alpn={} listener_started={}",
        status.enabled, status.listen, status.alpn, status.listener_started
    );
    let mut checks = vec![DoctorCheck::pass("quic runtime config", detail)];
    if !status.enabled {
        checks.push(DoctorCheck::fail(
            "quic runtime enabled",
            "server reports QUIC disabled; set WEBCODEX_QUIC_ENABLED=true and restart webcodex",
        ));
        return (checks, false);
    }
    if !status.listener_started {
        checks.push(DoctorCheck::fail(
            "quic listener started",
            format!(
                "server reports QUIC enabled but listener not started{}",
                status
                    .last_error
                    .as_deref()
                    .map(|e| format!(": {}", e))
                    .unwrap_or_default()
            ),
        ));
        return (checks, false);
    }
    (checks, true)
}

fn read_doctor_agent_config(path: &Path) -> Result<DoctorAgentConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read agent config {}: {}", path.display(), e))?;
    toml::from_str(&content)
        .map_err(|e| format!("failed to parse agent config {}: {}", path.display(), e))
}

pub(crate) fn resolve_doctor_quic_options(
    opts: &DoctorOptions,
) -> Result<DoctorQuicResolved, String> {
    let agent_cfg = match opts.agent_config.as_deref() {
        Some(path) => Some(read_doctor_agent_config(path)?),
        None => None,
    };
    let quic_cfg = agent_cfg.as_ref().and_then(|cfg| cfg.quic.as_ref());
    let server_addr = opts
        .quic_server_addr
        .clone()
        .or_else(|| quic_cfg.map(|q| q.server_addr.clone()))
        .unwrap_or_default();
    let server_name = opts
        .quic_server_name
        .clone()
        .or_else(|| quic_cfg.map(|q| q.server_name.clone()))
        .unwrap_or_default();
    let alpn = if opts.quic_alpn.trim().is_empty() {
        quic_cfg
            .map(|q| q.alpn.clone())
            .unwrap_or_else(default_doctor_quic_alpn)
    } else {
        opts.quic_alpn.clone()
    };
    let timeout_secs = if opts.quic_timeout_secs == 0 {
        quic_cfg
            .map(|q| q.connect_timeout_secs)
            .filter(|v| *v > 0)
            .unwrap_or_else(default_doctor_quic_connect_timeout_secs)
    } else {
        opts.quic_timeout_secs
    };
    let client_id = opts.quic_client_id.clone().or_else(|| {
        agent_cfg.as_ref().and_then(|cfg| {
            let id = cfg.client_id.trim();
            if id.is_empty() {
                None
            } else {
                Some(id.to_string())
            }
        })
    });
    if server_addr.trim().is_empty() {
        return Err(
            "--quic-server-addr is required for --quic unless [quic].server_addr is in --agent-config"
                .to_string(),
        );
    }
    if server_name.trim().is_empty() {
        return Err(
            "--quic-server-name is required for --quic unless [quic].server_name is in --agent-config"
                .to_string(),
        );
    }
    Ok(DoctorQuicResolved {
        server_addr,
        server_name,
        alpn,
        timeout_secs,
        client_id,
    })
}

fn classify_quic_connect_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("certificate")
        || lower.contains("cert")
        || lower.contains("webpki")
        || lower.contains("notvalidforname")
        || lower.contains("unknownissuer")
    {
        "certificate verify failed (check server_name and certificate SAN/issuer)".to_string()
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "connect timeout (check UDP firewall/security group/NAT and listener bind)".to_string()
    } else if lower.contains("applicationclosed")
        || lower.contains("connectionclosed")
        || lower.contains("closed")
        || lower.contains("no application protocol")
        || lower.contains("alpn")
    {
        "handshake failed (check QUIC listener is enabled and ALPN matches)".to_string()
    } else {
        "quic connect failed".to_string()
    }
}

async fn doctor_quic_handshake(
    addr: SocketAddr,
    server_name: &str,
    alpn: &str,
    timeout_secs: u64,
) -> Result<(), String> {
    let client_crypto = build_doctor_quic_client_crypto(alpn)?;
    let client_config = quinn::ClientConfig::new(Arc::new(client_crypto));
    let endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().expect("valid local addr"))
        .map_err(|e| format!("failed to bind local quic UDP socket: {}", e))?;
    let connect = endpoint
        .connect_with(client_config, addr, server_name)
        .map_err(|e| format!("failed to start quic connect: {}", e))?;
    let conn = tokio::time::timeout(Duration::from_secs(timeout_secs), connect)
        .await
        .map_err(|_| format!("quic connect to {} timed out after {}s", addr, timeout_secs))?
        .map_err(|e| {
            let raw = e.to_string();
            format!("{}: {}", classify_quic_connect_error(&raw), raw)
        })?;
    conn.close(0u32.into(), b"webcodex doctor done");
    endpoint.wait_idle().await;
    Ok(())
}

pub(crate) async fn run_quic_doctor_checks(
    opts: &DoctorOptions,
    preferred_token: Option<&str>,
) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let resolved = match resolve_doctor_quic_options(opts) {
        Ok(resolved) => resolved,
        Err(e) => {
            checks.push(DoctorCheck::fail("quic config", e));
            return checks;
        }
    };
    checks.push(DoctorCheck::pass(
        "quic config",
        format!(
            "server_addr={} server_name={} alpn={} timeout_secs={} client_id={}",
            resolved.server_addr,
            resolved.server_name,
            resolved.alpn,
            resolved.timeout_secs,
            resolved.client_id.as_deref().unwrap_or("(not specified)")
        ),
    ));

    if let (Some(server_url), Some(token)) = (opts.server_url.as_deref(), preferred_token) {
        match http_post_json_status(server_url, "/api/runtime/status", Some(token), json!({})).await
        {
            Ok((status, _, Some(value))) if (200..300).contains(&status) => {
                let output = value.get("output").unwrap_or(&value);
                let (mut runtime_checks, should_continue) = doctor_runtime_quic_checks(output);
                checks.append(&mut runtime_checks);
                if !should_continue {
                    return checks;
                }
            }
            Ok((status, content_type, _)) => checks.push(DoctorCheck::warn(
                "quic runtime config",
                format!(
                    "runtime_status unavailable for QUIC preflight: HTTP {} content-type {}",
                    status, content_type
                ),
            )),
            Err(e) => checks.push(DoctorCheck::warn(
                "quic runtime config",
                format!("runtime_status unavailable for QUIC preflight: {}", e),
            )),
        }
    } else {
        checks.push(DoctorCheck::warn(
            "quic runtime config",
            "not checked; pass --server-url with --user-token-file or --token-file to read runtime_status",
        ));
    }

    let addrs = match resolved.server_addr.to_socket_addrs() {
        Ok(iter) => iter.collect::<Vec<_>>(),
        Err(e) => {
            checks.push(DoctorCheck::fail(
                "quic resolve",
                format!("failed to resolve {}: {}", resolved.server_addr, e),
            ));
            return checks;
        }
    };
    if addrs.is_empty() {
        checks.push(DoctorCheck::fail(
            "quic resolve",
            format!("{} resolved to no socket addresses", resolved.server_addr),
        ));
        return checks;
    }
    checks.push(DoctorCheck::pass(
        "quic resolve",
        format!(
            "{} -> {}",
            resolved.server_addr,
            addrs
                .iter()
                .map(SocketAddr::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ));

    let mut handshake_ok = false;
    let mut handshake_errors = Vec::new();
    for addr in &addrs {
        match doctor_quic_handshake(
            *addr,
            &resolved.server_name,
            &resolved.alpn,
            resolved.timeout_secs,
        )
        .await
        {
            Ok(()) => {
                checks.push(DoctorCheck::pass(
                    "quic handshake",
                    format!(
                        "{} ok; ALPN '{}' negotiated and certificate SAN/chain verified for {}",
                        addr, resolved.alpn, resolved.server_name
                    ),
                ));
                handshake_ok = true;
                break;
            }
            Err(e) => handshake_errors.push(format!("{}: {}", addr, e)),
        }
    }
    if !handshake_ok {
        checks.push(DoctorCheck::fail(
            "quic handshake",
            format!(
                "server listener started but UDP handshake failed: {}",
                handshake_errors.join("; ")
            ),
        ));
        return checks;
    }

    if opts.quic_server_only || !opts.quic_agent_e2e {
        checks.push(DoctorCheck::warn(
            "quic agent e2e",
            "skipped; pass --agent-e2e with --server-url, --user-token-file, --project, and an online quic-v1 agent",
        ));
        return checks;
    }

    let Some(server_url) = opts.server_url.as_deref() else {
        checks.push(DoctorCheck::fail(
            "quic agent e2e",
            "--server-url is required for --agent-e2e",
        ));
        return checks;
    };
    let Some(token) = preferred_token else {
        checks.push(DoctorCheck::fail(
            "quic agent e2e",
            "--user-token-file or --token-file/--env-file is required for runtime API checks",
        ));
        return checks;
    };
    let Some(project) = opts.project.as_deref() else {
        checks.push(DoctorCheck::fail(
            "quic agent e2e",
            "--project is required for run_shell/run_job checks",
        ));
        return checks;
    };

    checks.extend(
        run_quic_agent_e2e_checks(server_url, token, project, resolved.client_id.as_deref()).await,
    );
    checks
}

async fn run_quic_agent_e2e_checks(
    server_url: &str,
    token: &str,
    project: &str,
    expected_client_id: Option<&str>,
) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    match http_post_json_status(server_url, "/api/runtime/status", Some(token), json!({})).await {
        Ok((status, _, Some(value))) if (200..300).contains(&status) => {
            let output = value.get("output").unwrap_or(&value);
            let clients = output
                .pointer("/agents/clients")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let matching = clients.iter().find(|client| {
                let transport = client.get("transport").and_then(Value::as_str);
                let protocol = client.get("agent_protocol_version").and_then(Value::as_str);
                let client_id_matches = expected_client_id
                    .is_none_or(|id| client.get("client_id").and_then(Value::as_str) == Some(id));
                client_id_matches && transport == Some("quic") && protocol == Some("quic-v1")
            });
            let wrong_transport_or_protocol = clients.iter().find(|client| {
                let client_id_matches = expected_client_id
                    .is_none_or(|id| client.get("client_id").and_then(Value::as_str) == Some(id));
                let connected = client.get("connected").and_then(Value::as_bool) == Some(true);
                let transport = client.get("transport").and_then(Value::as_str);
                let protocol = client.get("agent_protocol_version").and_then(Value::as_str);
                client_id_matches
                    && connected
                    && (transport != Some("quic") || protocol != Some("quic-v1"))
            });
            match matching {
                Some(client) => {
                    let connected = client.get("connected").and_then(Value::as_bool);
                    let pending = client.get("pending_requests").and_then(Value::as_u64);
                    checks.push(DoctorCheck::pass(
                        "quic agent online",
                        format!(
                            "client_id={} transport=quic protocol=quic-v1 connected={:?} pending_requests={} last_seen={}",
                            client
                                .get("client_id")
                                .and_then(Value::as_str)
                                .unwrap_or("(unknown)"),
                            connected,
                            pending
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "unknown".to_string()),
                            client
                                .get("last_seen")
                                .map(Value::to_string)
                                .unwrap_or_else(|| "unknown".to_string())
                        ),
                    ));
                    if connected != Some(true) {
                        checks.push(DoctorCheck::fail(
                            "quic agent connected",
                            "matching quic-v1 agent is not connected",
                        ));
                    }
                    if !client
                        .get("capabilities")
                        .is_some_and(|cap| cap.is_object() && cap.get("shell").is_some())
                    {
                        checks.push(DoctorCheck::warn(
                            "quic capabilities",
                            "matching agent did not expose a shell capability summary",
                        ));
                    }
                }
                None => checks.push(DoctorCheck::fail(
                    "quic agent online",
                    if let Some(client) = wrong_transport_or_protocol {
                        format!(
                            "agent online but wrong protocol/transport: client_id={} transport={} protocol={}",
                            client
                                .get("client_id")
                                .and_then(Value::as_str)
                                .unwrap_or("(unknown)"),
                            client
                                .get("transport")
                                .and_then(Value::as_str)
                                .unwrap_or("(missing)"),
                            client
                                .get("agent_protocol_version")
                                .and_then(Value::as_str)
                                .unwrap_or("(missing)")
                        )
                    } else {
                        match expected_client_id {
                            Some(id) => {
                                format!("no online quic-v1 agent found for client_id={}", id)
                            }
                            None => "no online quic-v1 agent found".to_string(),
                        }
                    },
                )),
            }
        }
        Ok((status, content_type, _)) => checks.push(DoctorCheck::fail(
            "quic agent online",
            format!(
                "runtime_status HTTP {} content-type {}",
                status, content_type
            ),
        )),
        Err(e) => checks.push(DoctorCheck::fail("quic agent online", e)),
    }

    match http_post_json_status(
        server_url,
        "/api/tools/call",
        Some(token),
        json!({"tool":"run_shell","params":{"project":project,"command":"printf webcodex-quic-ok"}}),
    )
    .await
    {
        Ok((status, _, Some(value))) if (200..300).contains(&status) => {
            let stdout = value
                .pointer("/output/stdout")
                .and_then(Value::as_str)
                .unwrap_or("");
            let exit_code = value.pointer("/output/exit_code").and_then(Value::as_i64);
            if stdout.contains("webcodex-quic-ok") && exit_code == Some(0) {
                checks.push(DoctorCheck::pass(
                    "quic run_shell",
                    format!("project '{}' returned marker", project),
                ));
            } else {
                checks.push(DoctorCheck::fail(
                    "quic run_shell",
                    format!(
                        "project '{}' exit_code={:?} without expected marker",
                        project, exit_code
                    ),
                ));
            }
        }
        Ok((status, content_type, _)) => checks.push(DoctorCheck::fail(
            "quic run_shell",
            format!("HTTP {} content-type {}", status, content_type),
        )),
        Err(e) => checks.push(DoctorCheck::fail("quic run_shell", e)),
    }

    let job_id = match http_post_json_status(
        server_url,
        "/api/tools/call",
        Some(token),
        json!({"tool":"run_job","params":{"project":project,"command":"printf webcodex-quic-job-ok","timeout_secs":10}}),
    )
    .await
    {
        Ok((status, _, Some(value))) if (200..300).contains(&status) => {
            match value.pointer("/output/job_id").and_then(Value::as_str) {
                Some(job_id) => {
                    checks.push(DoctorCheck::pass(
                        "quic run_job",
                        format!("started job_id={}", job_id),
                    ));
                    Some(job_id.to_string())
                }
                None => {
                    checks.push(DoctorCheck::fail(
                        "quic run_job",
                        "response did not include output.job_id",
                    ));
                    None
                }
            }
        }
        Ok((status, content_type, _)) => {
            checks.push(DoctorCheck::fail(
                "quic run_job",
                format!("HTTP {} content-type {}", status, content_type),
            ));
            None
        }
        Err(e) => {
            checks.push(DoctorCheck::fail("quic run_job", e));
            None
        }
    };

    let Some(job_id) = job_id else {
        return checks;
    };
    let mut final_status = None;
    for _ in 0..20 {
        match http_post_json_status(
            server_url,
            "/api/tools/call",
            Some(token),
            json!({"tool":"job_status","params":{"job_id":job_id}}),
        )
        .await
        {
            Ok((status, _, Some(value))) if (200..300).contains(&status) => {
                let status = value
                    .pointer("/output/status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if matches!(status.as_str(), "completed" | "failed" | "stopped" | "lost") {
                    final_status = Some(status);
                    break;
                }
            }
            Ok((status, content_type, _)) => {
                checks.push(DoctorCheck::fail(
                    "quic job_status",
                    format!("HTTP {} content-type {}", status, content_type),
                ));
                return checks;
            }
            Err(e) => {
                checks.push(DoctorCheck::fail("quic job_status", e));
                return checks;
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    match final_status.as_deref() {
        Some("completed") => checks.push(DoctorCheck::pass(
            "quic job_status",
            format!("job_id={} completed", job_id),
        )),
        Some(status) => checks.push(DoctorCheck::fail(
            "quic job_status",
            format!("job_id={} ended with status={}", job_id, status),
        )),
        None => checks.push(DoctorCheck::fail(
            "quic job_status",
            format!("job_id={} did not finish before timeout", job_id),
        )),
    }

    match http_post_json_status(
        server_url,
        "/api/tools/call",
        Some(token),
        json!({"tool":"job_log","params":{"job_id":job_id,"tail_lines":50}}),
    )
    .await
    {
        Ok((status, _, Some(value))) if (200..300).contains(&status) => {
            let stdout = value
                .pointer("/output/stdout")
                .and_then(Value::as_str)
                .unwrap_or("");
            if stdout.contains("webcodex-quic-job-ok") {
                checks.push(DoctorCheck::pass(
                    "quic job_log",
                    format!("job_id={} output marker found", job_id),
                ));
            } else {
                checks.push(DoctorCheck::fail(
                    "quic job_log",
                    format!("job_id={} output marker missing", job_id),
                ));
            }
        }
        Ok((status, content_type, _)) => checks.push(DoctorCheck::fail(
            "quic job_log",
            format!("HTTP {} content-type {}", status, content_type),
        )),
        Err(e) => checks.push(DoctorCheck::fail("quic job_log", e)),
    }
    checks.push(DoctorCheck::warn(
        "quic disconnect",
        "manual step: stop the agent and rerun runtime_status/list_agents to observe stale/offline reconciliation",
    ));
    checks
}

// Local agent-config doctor (shell profiles / projects). Parses agent.toml
// locally without contacting the server and never reports init_script bodies,
// env values, tokens, or the full env snapshot.

#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
struct DoctorShellProfileConfig {
    #[serde(default)]
    program: Option<String>,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    init_script: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DoctorShellConfig {
    #[serde(default)]
    default_profile: Option<String>,
    #[serde(default)]
    profiles: BTreeMap<String, DoctorShellProfileConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
struct DoctorAgentPolicy {
    #[serde(default)]
    allowed_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct DoctorAgentConfig {
    #[serde(default)]
    server_url: String,
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    transport: Option<String>,
    #[serde(default)]
    quic: Option<DoctorQuicConfig>,
    #[serde(default)]
    projects_dir: Option<PathBuf>,
    #[serde(default)]
    shell: DoctorShellConfig,
    #[serde(default)]
    policy: DoctorAgentPolicy,
}

#[derive(Debug, Clone, Deserialize)]
struct DoctorQuicConfig {
    #[serde(default)]
    server_addr: String,
    #[serde(default)]
    server_name: String,
    #[serde(default = "default_doctor_quic_alpn")]
    alpn: String,
    #[serde(default = "default_doctor_quic_connect_timeout_secs")]
    connect_timeout_secs: u64,
}

fn default_doctor_quic_alpn() -> String {
    "webcodex-agent/1".to_string()
}

fn default_doctor_quic_connect_timeout_secs() -> u64 {
    10
}

#[derive(Debug, Clone, Deserialize)]
struct DoctorAgentProject {
    id: String,
    path: String,
    #[serde(default)]
    shell_profile: Option<String>,
    #[serde(default)]
    disabled: bool,
}

fn resolve_doctor_profile_name(
    project: &DoctorAgentProject,
    shell: &DoctorShellConfig,
) -> Option<String> {
    project
        .shell_profile
        .clone()
        .or_else(|| shell.default_profile.clone())
}

pub(crate) fn run_local_agent_doctor(config_path: &Path) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let content = match std::fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(e) => {
            checks.push(DoctorCheck::fail(
                "agent config",
                format!("failed to read {}: {}", config_path.display(), e),
            ));
            return checks;
        }
    };
    let cfg: DoctorAgentConfig = match toml::from_str(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            checks.push(DoctorCheck::fail(
                "agent config",
                format!("failed to parse {}: {}", config_path.display(), e),
            ));
            return checks;
        }
    };
    checks.push(DoctorCheck::pass(
        "agent config",
        format!(
            "parsed {}; client_id={}",
            config_path.display(),
            if cfg.client_id.trim().is_empty() {
                "(empty)"
            } else {
                cfg.client_id.as_str()
            }
        ),
    ));

    let configured_count = cfg.shell.profiles.len();
    let profile_names: Vec<&str> = cfg.shell.profiles.keys().map(String::as_str).collect();
    checks.push(DoctorCheck::pass(
        "shell profiles",
        format!(
            "configured_count={} default_profile={} profiles=[{}]",
            configured_count,
            cfg.shell.default_profile.as_deref().unwrap_or("(none)"),
            profile_names.join(", ")
        ),
    ));
    if let Some(default_profile) = &cfg.shell.default_profile {
        if !cfg.shell.profiles.contains_key(default_profile) {
            checks.push(DoctorCheck::fail(
                "shell default_profile",
                format!(
                    "shell.default_profile '{}' does not match any shell.profiles entry",
                    default_profile
                ),
            ));
        }
    }

    let projects_dir = cfg.projects_dir.clone().unwrap_or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config/webcodex/projects.d")
    });
    if !projects_dir.exists() {
        checks.push(DoctorCheck::warn(
            "projects_dir",
            format!("{} does not exist", projects_dir.display()),
        ));
        return checks;
    }
    let mut project_files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&projects_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
                project_files.push(path);
            }
        }
    }
    project_files.sort();
    let mut loaded = 0usize;
    let mut parse_errors = 0usize;
    for file in &project_files {
        let content = match std::fs::read_to_string(file) {
            Ok(content) => content,
            Err(e) => {
                parse_errors += 1;
                checks.push(DoctorCheck::warn(
                    "project config",
                    format!("failed to read {}: {}", file.display(), e),
                ));
                continue;
            }
        };
        let project: DoctorAgentProject = match toml::from_str(&content) {
            Ok(project) => project,
            Err(e) => {
                parse_errors += 1;
                checks.push(DoctorCheck::warn(
                    "project config",
                    format!("failed to parse {}: {}", file.display(), e),
                ));
                continue;
            }
        };
        if project.disabled {
            continue;
        }
        loaded += 1;
        let project_path = PathBuf::from(&project.path);
        if !project_path.exists() {
            checks.push(DoctorCheck::fail(
                format!("project '{}' path", project.id),
                format!("path {} does not exist", project_path.display()),
            ));
        }
        if !cfg.policy.allowed_roots.is_empty() {
            let inside = match project_path.canonicalize() {
                Ok(canon) => cfg.policy.allowed_roots.iter().any(|root| {
                    root.canonicalize()
                        .map(|root| canon == root || canon.starts_with(&root))
                        .unwrap_or(false)
                }),
                Err(_) => false,
            };
            if !inside {
                checks.push(DoctorCheck::warn(
                    format!("project '{}' allowed_roots", project.id),
                    format!(
                        "path {} is outside the configured allowed_roots",
                        project_path.display()
                    ),
                ));
            }
        }
        let resolved = resolve_doctor_profile_name(&project, &cfg.shell);
        match resolved {
            None => checks.push(DoctorCheck::pass(
                format!("project '{}' shell_profile", project.id),
                "no profile configured (fallback to plain shell)".to_string(),
            )),
            Some(name) => {
                if cfg.shell.profiles.contains_key(&name) {
                    let profile = cfg.shell.profiles.get(&name).expect("checked above");
                    checks.push(DoctorCheck::pass(
                        format!("project '{}' shell_profile", project.id),
                        format!(
                            "resolved='{}' has_init_script={} env_keys_count={}",
                            name,
                            profile.init_script.is_some(),
                            profile.env.len()
                        ),
                    ));
                } else {
                    checks.push(DoctorCheck::fail(
                        format!("project '{}' shell_profile", project.id),
                        format!(
                            "resolved profile '{}' is not in shell.profiles (project.shell_profile={}, default_profile={})",
                            name,
                            project.shell_profile.as_deref().unwrap_or("(none)"),
                            cfg.shell.default_profile.as_deref().unwrap_or("(none)")
                        ),
                    ));
                }
            }
        }
    }
    checks.push(DoctorCheck::pass(
        "projects_dir",
        format!(
            "{} loaded={} parse_errors={}",
            projects_dir.display(),
            loaded,
            parse_errors
        ),
    ));
    checks
}
