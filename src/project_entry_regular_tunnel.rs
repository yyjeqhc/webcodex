use super::client_handoff_service::{copy_text_to_clipboard, mcp_url, ClipboardCopyOutcome};
use super::openai_tunnel_service::{prepare_openai_tunnel, start_openai_tunnel_with_stop};
use super::setup_service::create_private_dir;
use super::ProductError;
use serde_json::{json, Value};
use std::future::Future;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
#[path = "project_entry_regular_tunnel/health_events.rs"]
mod health_events;

const REGULAR_TUNNEL_STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegularServerTunnelOptions {
    pub(crate) local_server_url: String,
    pub(crate) bootstrap_token: String,
    pub(crate) runtime_parent: PathBuf,
    pub(crate) stop_on_stdin_eof: bool,
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
}

impl Drop for RegularTunnelSession {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

pub(crate) async fn run_regular_server_tunnel_with_stop(
    options: &RegularServerTunnelOptions,
    stop: impl Future<Output = ()>,
) -> Result<(), ProductError> {
    match run_regular_server_tunnel_inner(options, stop).await {
        Ok(()) => Ok(()),
        Err(error) => {
            println!("{}", machine_regular_tunnel_failure_event(&error));
            Err(error)
        }
    }
}

async fn run_regular_server_tunnel_inner(
    options: &RegularServerTunnelOptions,
    stop: impl Future<Output = ()>,
) -> Result<(), ProductError> {
    let local_server_url = validate_local_server_url(&options.local_server_url)?;
    let session = RegularTunnelSession::create(&options.runtime_parent)?;
    // The parent lease covers preparation and connection startup too. Waiting
    // until readiness to observe EOF can leave the owner fenced on Windows exit.
    let stop = async {
        tokio::select! {
            _ = stop => {},
            _ = wait_for_regular_tunnel_stop_signal(options.stop_on_stdin_eof) => {},
        }
    };
    tokio::pin!(stop);
    let prerequisites = tokio::select! {
        biased;
        _ = &mut stop => return Ok(()),
        result = prepare_openai_tunnel() => result?,
    };
    let deadline = Instant::now() + REGULAR_TUNNEL_STARTUP_TIMEOUT;
    let mcp_endpoint = mcp_url(&local_server_url);
    let Some(mut tunnel) = start_openai_tunnel_with_stop(
        &prerequisites,
        &mcp_endpoint,
        &options.bootstrap_token,
        deadline,
        &mut stop,
    )
    .await?
    else {
        return Ok(());
    };

    // Managed profiles use explicit Copy ID controls; concurrent starts must not
    // race over the user's clipboard. Keep CLI handoff for an unmanaged invocation.
    let managed = std::env::var("WEBCODEX_TUNNEL_PROFILE_ID").is_ok();
    let clipboard = copy_text_to_clipboard(&prerequisites.tunnel_id, !managed).await;
    let mut ready = machine_regular_tunnel_ready_event(clipboard);
    ready["runtime"] = json!({
        "directory": session.directory,
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

    let health = tunnel.health();
    let local_mcp_url = mcp_url(&local_server_url);
    let readiness_path = options.runtime_parent.join("readiness.json");
    let service_readiness = managed
        .then_some(readiness_path)
        .filter(|path| path.is_file());
    let outcome = tokio::select! {
        _ = &mut stop => Ok(()),
        result = tunnel.wait_for_exit() => result,
        result = report_regular_tunnel_health(&health, &local_mcp_url, &options.bootstrap_token, service_readiness.as_deref(), options.stop_on_stdin_eof) => result,
    };
    if let Some(path) = service_readiness {
        let _ = webcodex_environment::write_tunnel_health(&path, false, false);
    }
    let stopped = tunnel.stop_with_outcome().await;
    outcome.and(stopped)
}

/// A running daemon is not sufficient proof of a usable local MCP endpoint.
/// No response body, credential, or network error text crosses the machine channel.
async fn report_regular_tunnel_health(
    health: &webcodex_openai_tunnel::Health,
    local_mcp_url: &str,
    bootstrap: &str,
    service_readiness: Option<&Path>,
    parent_heartbeat: bool,
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
    let mut events = health_events::HealthEvents::new(parent_heartbeat);
    loop {
        interval.tick().await;
        let tunnel_ready = health.is_ready();
        let local_mcp_ready = probe_local_mcp(&client, local_mcp_url, bootstrap).await;
        if let Some(path) = service_readiness {
            webcodex_environment::write_tunnel_health(path, tunnel_ready, local_mcp_ready)
                .map_err(|_| tunnel_auth_error("Could not persist service connection health"))?;
        }
        if events.should_emit(Instant::now(), tunnel_ready, local_mcp_ready) {
            println!(
                "{}",
                json!({"event":"health", "schema_version":1, "tunnel_ready":tunnel_ready, "local_mcp_ready":local_mcp_ready})
            );
        }
    }
}

pub(crate) async fn probe_local_mcp(
    client: &reqwest::Client,
    local_mcp_url: &str,
    bootstrap: &str,
) -> bool {
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
        "schema_version": 2,
        "provider": "openai",
        "ready_for_chatgpt": true,
        "client_connection": "not_observed",
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
        "tunnel_auth_rejected" => ("tunnel_control_plane", "tunnel_auth_rejected"),
        "tunnel_restart_uncertain" => ("tunnel_recovery", "tunnel_restart_uncertain"),
        "tunnel_capacity_exhausted" => ("tunnel_capacity", "tunnel_capacity_exhausted"),
        "tunnel_protocol_failed" => ("tunnel_protocol", "tunnel_protocol_failed"),
        "tunnel_control_plane_unreachable" => {
            ("tunnel_control_plane", "tunnel_control_plane_unreachable")
        }
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

pub(crate) async fn wait_for_regular_tunnel_stop_signal(stop_on_stdin_eof: bool) {
    wait_for_regular_tunnel_stop_signal_with(
        stop_on_stdin_eof,
        wait_for_platform_stop_signal(),
        wait_for_stdin_eof(),
    )
    .await;
}

async fn wait_for_regular_tunnel_stop_signal_with<S, E>(
    stop_on_stdin_eof: bool,
    stop_signal: S,
    stdin_eof: E,
) where
    S: Future<Output = ()>,
    E: Future<Output = ()>,
{
    if stop_on_stdin_eof {
        tokio::select! {
            _ = stop_signal => {},
            _ = stdin_eof => {},
        }
    } else {
        // Do not even poll stdin in persistent mode: daemons commonly inherit
        // an already-closed stdin descriptor.
        stop_signal.await;
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
    use tokio::signal::unix::{signal, SignalKind};

    let Ok(mut terminate) = signal(SignalKind::terminate()) else {
        let _ = tokio::signal::ctrl_c().await;
        return;
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = terminate.recv() => {},
    }
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
#[path = "project_entry_regular_tunnel_startup_tests.rs"]
mod startup_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn persistent_tunnel_does_not_poll_stdin_eof() {
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(wait_for_regular_tunnel_stop_signal_with(
            false,
            async move {
                let _ = stop_rx.await;
            },
            async { panic!("persistent tunnel polled stdin EOF") },
        ));
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        stop_tx.send(()).unwrap();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn parent_liveness_mode_stops_on_stdin_eof() {
        let (_stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::time::timeout(
            Duration::from_secs(1),
            wait_for_regular_tunnel_stop_signal_with(
                true,
                async move {
                    let _ = stop_rx.await;
                },
                std::future::ready(()),
            ),
        )
        .await
        .expect("stdin EOF should stop parent-owned tunnel mode");
    }

    #[test]
    fn machine_ready_event_contains_only_safe_handoff_metadata() {
        let event = machine_regular_tunnel_ready_event(ClipboardCopyOutcome::Copied);
        assert_eq!(event["schema_version"], 2);
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
    fn tunnel_readiness_is_not_clipboard_delivery_or_verified_client_use() {
        for outcome in [
            ClipboardCopyOutcome::Copied,
            ClipboardCopyOutcome::Unavailable,
            ClipboardCopyOutcome::Disabled,
        ] {
            let value = machine_regular_tunnel_ready_event(outcome);
            assert_eq!(value["ready_for_chatgpt"], true);
            assert_eq!(value["client_connection"], "not_observed");
            assert!(value["connection"]["clipboard_state"].is_string());
        }
    }

    #[test]
    fn machine_failure_event_is_typed_bounded_and_secret_free() {
        let error = ProductError::new(
            "tunnel_auth_rejected",
            "private runtime key and tunnel id must never cross the machine channel",
            Some("private recovery text"),
        );
        let event = machine_regular_tunnel_failure_event(&error);
        assert_eq!(event["event"], "failure");
        assert_eq!(event["failure_stage"], "tunnel_control_plane");
        assert_eq!(event["reason_code"], "tunnel_auth_rejected");
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
    fn regular_tunnel_session_directory_is_distinct_and_cleaned_up() {
        let temp = tempfile::tempdir().unwrap();
        let first = RegularTunnelSession::create(temp.path()).unwrap();
        let second = RegularTunnelSession::create(temp.path()).unwrap();
        assert_ne!(first.directory, second.directory);
        let directory = first.directory.clone();
        drop(first);
        assert!(!directory.exists());
        assert!(second.directory.is_dir());
    }
}
