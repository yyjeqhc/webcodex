use super::client_handoff_service::{copy_text_to_clipboard, mcp_url, ClipboardCopyOutcome};
use super::openai_tunnel_service::{prepare_openai_tunnel, start_openai_tunnel};
use super::setup_service::{create_private_dir, write_new_private};
use super::ProductError;
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const REGULAR_TUNNEL_STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegularServerTunnelOptions {
    pub(crate) local_server_url: String,
    pub(crate) bootstrap_token: String,
    pub(crate) runtime_parent: PathBuf,
}

struct RegularTunnelSession {
    directory: PathBuf,
}

impl RegularTunnelSession {
    fn create(runtime_parent: &Path) -> Result<Self, ProductError> {
        let root = runtime_parent.join("regular-tunnel-runtime");
        create_private_dir(&root)?;
        let directory = root.join(format!("openai-{}", uuid::Uuid::new_v4().simple()));
        create_private_dir(&directory)?;
        Ok(Self { directory })
    }

    fn write_authorization_file(&self, bootstrap_token: &str) -> Result<PathBuf, ProductError> {
        let token = bootstrap_token.trim();
        if token.is_empty() {
            return Err(tunnel_auth_error(
                "the local Server bootstrap credential is unavailable",
            ));
        }
        let path = self.directory.join("openai-mcp-authorization");
        write_new_private(&path, format!("Bearer {token}").as_bytes())?;
        Ok(path)
    }
}

impl Drop for RegularTunnelSession {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

pub(crate) async fn run_regular_server_tunnel(
    options: &RegularServerTunnelOptions,
) -> Result<(), ProductError> {
    match run_regular_server_tunnel_inner(options).await {
        Ok(()) => Ok(()),
        Err(error) => {
            println!("{}", machine_regular_tunnel_failure_event(&error));
            Err(error)
        }
    }
}

async fn run_regular_server_tunnel_inner(
    options: &RegularServerTunnelOptions,
) -> Result<(), ProductError> {
    let local_server_url = validate_local_server_url(&options.local_server_url)?;
    let session = RegularTunnelSession::create(&options.runtime_parent)?;
    let authorization_file = session.write_authorization_file(&options.bootstrap_token)?;
    let prerequisites = prepare_openai_tunnel().await?;
    let deadline = Instant::now() + REGULAR_TUNNEL_STARTUP_TIMEOUT;
    let mut tunnel = start_openai_tunnel(
        &prerequisites,
        &mcp_url(&local_server_url),
        &authorization_file,
        &session.directory,
        deadline,
    )
    .await?;

    // Managed profiles use explicit Copy ID controls; concurrent starts must not
    // race over the user's clipboard. Keep CLI handoff for an unmanaged invocation.
    let managed = std::env::var("WEBCODEX_TUNNEL_PROFILE_ID").is_ok();
    let clipboard = copy_text_to_clipboard(&prerequisites.tunnel_id, !managed).await;
    let mut ready = machine_regular_tunnel_ready_event(clipboard);
    ready["runtime"] = json!({
        "directory": session.directory,
        "health_url": tunnel.health_url,
        "log_file": tunnel.log_file,
        "tunnel_client_pid": tunnel.pid(),
        "local_mcp_url": mcp_url(&local_server_url),
    });
    let encoded = serde_json::to_string(&ready).map_err(|_| {
        ProductError::new(
            "machine_output_failed",
            "WebCodex could not encode regular Tunnel readiness",
            Some("Retry the OpenAI Secure Tunnel."),
        )
    })?;
    println!("{encoded}");

    let health_url = tunnel.health_url.clone();
    let local_mcp_url = mcp_url(&local_server_url);
    let outcome = tokio::select! {
        _ = wait_for_regular_tunnel_stop_signal() => Ok(()),
        result = tunnel.wait_for_exit() => result,
        result = report_regular_tunnel_health(&health_url, &local_mcp_url, &options.bootstrap_token) => result,
    };
    tunnel.stop().await;
    outcome
}

/// A running daemon is not sufficient proof of a usable local MCP endpoint.
/// No response body, credential, or network error text crosses the machine channel.
async fn report_regular_tunnel_health(
    health_url: &str,
    local_mcp_url: &str,
    bootstrap: &str,
) -> Result<(), ProductError> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(1))
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| tunnel_auth_error("Local connection health monitoring is unavailable"))?;
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let (tunnel_ready, local_mcp_ready) = tokio::join!(
            async {
                client
                    .get(format!("{health_url}/readyz"))
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success())
            },
            probe_local_mcp(&client, local_mcp_url, bootstrap),
        );
        println!(
            "{}",
            json!({"event":"health", "schema_version":1, "tunnel_ready":tunnel_ready, "local_mcp_ready":local_mcp_ready})
        );
    }
}

async fn probe_local_mcp(client: &reqwest::Client, local_mcp_url: &str, bootstrap: &str) -> bool {
    let Ok(mut response) = client
        .get(local_mcp_url)
        .bearer_auth(bootstrap.trim())
        .send()
        .await
    else {
        return false;
    };
    if !response.status().is_success() || response.content_length().is_some_and(|size| size > 8192)
    {
        return false;
    }
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if body.len() + chunk.len() <= 8192 => body.extend_from_slice(&chunk),
            Ok(None) => break,
            _ => return false,
        }
    }
    serde_json::from_slice::<Value>(&body).is_ok_and(|value| {
        value["name"] == "webcodex" && value["protocol"] == "mcp" && value["endpoint"] == "/mcp"
    })
}

fn machine_regular_tunnel_ready_event(clipboard: ClipboardCopyOutcome) -> Value {
    let clipboard_state = match clipboard {
        ClipboardCopyOutcome::Copied => "copied",
        ClipboardCopyOutcome::Unavailable => "unavailable",
        ClipboardCopyOutcome::Disabled => "disabled",
    };
    json!({
        "event": "ready",
        "schema_version": 1,
        "provider": "openai",
        "ready_for_chatgpt": clipboard == ClipboardCopyOutcome::Copied,
        "connection": {
            "kind": "openai_tunnel",
            "clipboard_state": clipboard_state,
            "clipboard_contains": "tunnel_id",
        }
    })
}

fn machine_regular_tunnel_failure_event(error: &ProductError) -> Value {
    let (failure_stage, reason_code) = tunnel_failure_evidence(&error.code);
    json!({
        "event": "failure",
        "schema_version": 1,
        "provider": "openai",
        "failure_stage": failure_stage,
        "reason_code": reason_code,
    })
}

fn tunnel_failure_evidence(code: &str) -> (&'static str, &'static str) {
    match code {
        "tunnel_client_verification_failed" => (
            "tunnel_client_verification",
            "tunnel_client_verification_failed",
        ),
        "tunnel_doctor_failed" => ("tunnel_doctor", "tunnel_doctor_failed"),
        "tunnel_control_plane_unreachable" => {
            ("tunnel_control_plane", "tunnel_control_plane_unreachable")
        }
        "tunnel_control_plane_probe_failed" => {
            ("tunnel_control_plane", "tunnel_control_plane_probe_failed")
        }
        "tunnel_daemon_start_failed" => ("tunnel_daemon_start", "tunnel_daemon_start_failed"),
        "tunnel_daemon_not_ready" => ("tunnel_daemon_readiness", "tunnel_daemon_not_ready"),
        "local_mcp_unavailable" | "tunnel_auth_invalid" => ("local_mcp", "local_mcp_unavailable"),
        _ => ("tunnel_startup", "tunnel_startup_failed"),
    }
}

fn validate_local_server_url(value: &str) -> Result<String, ProductError> {
    let value = value.trim().trim_end_matches('/');
    let parsed = url::Url::parse(value).map_err(|_| {
        ProductError::new(
            "unsupported_topology",
            "Regular OpenAI Tunnel requires a valid local WebCodex Server URL",
            Some("Start the local WebCodex runtime before starting the Tunnel."),
        )
    })?;
    let host = parsed.host_str().unwrap_or("");
    if parsed.scheme() != "http"
        || !matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
        || parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !matches!(parsed.path(), "" | "/")
    {
        return Err(ProductError::new(
            "unsupported_topology",
            "Regular OpenAI Tunnel only exposes a loopback local WebCodex Server",
            Some("Use this command with Local Full Runtime; remote Server exposure is managed remotely."),
        ));
    }
    Ok(value.to_string())
}

async fn wait_for_regular_tunnel_stop_signal() {
    tokio::select! {
        _ = wait_for_platform_stop_signal() => {},
        _ = wait_for_stdin_eof() => {},
    }
}

async fn wait_for_stdin_eof() {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = std::thread::Builder::new()
        .name("webcodex-server-tunnel-stdin".to_string())
        .spawn(move || {
            let mut stdin = std::io::stdin().lock();
            let mut buffer = [0_u8; 256];
            loop {
                match stdin.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            let _ = tx.send(());
        });
    let _ = rx.await;
}

#[cfg(not(windows))]
async fn wait_for_platform_stop_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(windows)]
async fn wait_for_platform_stop_signal() {
    let mut ctrl_break = match tokio::signal::windows::ctrl_break() {
        Ok(signal) => signal,
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = ctrl_break.recv() => {},
    }
}

fn tunnel_auth_error(message: &str) -> ProductError {
    ProductError::new(
        "tunnel_auth_invalid",
        message,
        Some("Restore the local Server bootstrap configuration, then retry."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_ready_event_contains_only_safe_handoff_metadata() {
        let event = machine_regular_tunnel_ready_event(ClipboardCopyOutcome::Copied);
        let encoded = serde_json::to_string(&event).unwrap();
        assert!(encoded.contains("\"provider\":\"openai\""));
        assert!(encoded.contains("\"clipboard_contains\":\"tunnel_id\""));
        assert!(!encoded.contains("CONTROL_PLANE_API_KEY"));
        assert!(!encoded.contains("Authorization"));
        assert!(!encoded.contains("Bearer"));
        assert!(!encoded.contains("wc_pat_"));
        assert!(!encoded.contains("wc_boot_"));
    }

    #[test]
    fn machine_failure_event_is_typed_bounded_and_secret_free() {
        let error = ProductError::new(
            "tunnel_control_plane_probe_failed",
            "private runtime key and tunnel id must never cross the machine channel",
            Some("private recovery text"),
        );
        let event = machine_regular_tunnel_failure_event(&error);
        assert_eq!(event["event"], "failure");
        assert_eq!(event["failure_stage"], "tunnel_control_plane");
        assert_eq!(event["reason_code"], "tunnel_control_plane_probe_failed");
        let encoded = serde_json::to_string(&event).unwrap();
        assert!(!encoded.contains("private runtime key"));
        assert!(!encoded.contains("private recovery"));
        assert!(!encoded.contains("Authorization"));
        assert!(!encoded.contains("Bearer"));
    }

    #[test]
    fn regular_tunnel_rejects_non_loopback_server_origins() {
        assert!(validate_local_server_url("http://127.0.0.1:8080").is_ok());
        assert!(validate_local_server_url("http://localhost:8080").is_ok());
        assert!(validate_local_server_url("https://example.test").is_err());
        assert!(validate_local_server_url("http://0.0.0.0:8080").is_err());
    }

    #[test]
    fn authorization_file_is_private_distinct_and_cleaned_up() {
        let temp = tempfile::tempdir().unwrap();
        let bootstrap_token = "wc_boot_test_secret";
        let session = RegularTunnelSession::create(temp.path()).unwrap();
        let session_dir = session.directory.clone();
        let authorization_file = session.write_authorization_file(bootstrap_token).unwrap();
        assert_eq!(
            std::fs::read_to_string(&authorization_file).unwrap(),
            format!("Bearer {bootstrap_token}")
        );
        drop(session);
        assert!(!session_dir.exists());
        assert!(!authorization_file.exists());
    }

    #[test]
    fn authorization_file_rejects_empty_bootstrap_credentials() {
        let temp = tempfile::tempdir().unwrap();
        for value in ["", "   "] {
            let session = RegularTunnelSession::create(temp.path()).unwrap();
            let error = session.write_authorization_file(value).unwrap_err();
            assert_eq!(error.code, "tunnel_auth_invalid");
            assert_eq!(
                error.message,
                "the local Server bootstrap credential is unavailable"
            );
            drop(session);
        }
    }
}
